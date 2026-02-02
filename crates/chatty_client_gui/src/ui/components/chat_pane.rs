use chatty_domain::RoomKey;
use iced::widget::{button, column, container, pane_grid, row, rule, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Task};
use rust_i18n::t;

use super::chat_message::ChatMessageView;
use crate::app::state::ConnectionStatus;
use crate::app::{Chatty, InsertTarget, Message};
use crate::theme::Palette;
use crate::ui::components::tab::{ChatItem, TabId, TabModel};

#[derive(Debug, Clone)]
pub enum ChatPaneMessage {
	ComposerChanged(String),
	SendPressed,
}

#[derive(Debug, Clone)]
pub struct ChatPane {
	pub tab_id: Option<TabId>,
	pub composer: String,
	pub join_raw: String,
	pub reply_to_server_message_id: String,
	pub reply_to_platform_message_id: String,
	pub reply_to_room: Option<RoomKey>,
}

impl ChatPane {
	pub fn new(tab_id: Option<TabId>) -> Self {
		Self {
			tab_id,
			composer: String::new(),
			join_raw: String::new(),
			reply_to_server_message_id: String::new(),
			reply_to_platform_message_id: String::new(),
			reply_to_room: None,
		}
	}

	pub fn update(&mut self, pane: pane_grid::Pane, message: ChatPaneMessage, app: &mut Chatty) -> Task<Message> {
		match message {
			ChatPaneMessage::ComposerChanged(v) => {
				self.composer = v;
				app.save_ui_layout();
				Task::none()
			}
			ChatPaneMessage::SendPressed => {
				let task = app.update_pane_send_pressed(pane);
				self.composer.clear();
				app.save_ui_layout();
				task
			}
		}
	}

	pub fn view<'a>(
		&'a self,
		app: &'a Chatty,
		tab: &'a TabModel,
		pane: pane_grid::Pane,
		palette: Palette,
	) -> pane_grid::Content<'a, Message> {
		let is_focused = Some(pane) == tab.focused_pane;
		let title = self
			.tab_id
			.and_then(|tid| app.state.tabs.get(&tid).map(|t| t.title.clone()))
			.unwrap_or_else(|| t!("main.welcome").to_string());

		let title_color = if is_focused { palette.text } else { palette.text_dim };
		let title_bar = pane_grid::TitleBar::new(text(title).color(title_color)).padding(6);

		let body: Element<'a, Message> = match self.tab_id.and_then(|tid| app.state.tabs.get(&tid)) {
			Some(tab_ref) => self.view_subscribed_pane(app, tab, pane, tab_ref, palette),
			None => self.view_unsubscribed_pane(app, pane, palette),
		};

		let pane_body = container(body)
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.chat_bg)),
				border: Border {
					color: palette.border,
					width: 1.0,
					radius: 8.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			});

		pane_grid::Content::new(pane_body).title_bar(title_bar)
	}

	fn view_subscribed_pane<'a>(
		&'a self,
		app: &'a Chatty,
		tab: &'a TabModel,
		pane: pane_grid::Pane,
		tab_ref: &'a TabModel,
		palette: Palette,
	) -> Element<'a, Message> {
		let rooms = tab_ref.target.0.clone();
		let can_send = rooms
			.iter()
			.any(|rk| app.state.room_permissions.get(rk).map(|p| p.can_send).unwrap_or(true));

		let connected = matches!(app.state.connection, ConnectionStatus::Connected { .. });
		let can_compose = connected && !rooms.is_empty() && can_send;

		let mut col = column![].spacing(4);

		let mut platforms = std::collections::HashSet::new();
		for room in &rooms {
			platforms.insert(room.platform);
		}
		for platform in platforms {
			let has_identity = app.state.gui_settings().identities.iter().any(|id| id.platform == platform);
			if !has_identity {
				let warning_text = match platform {
					chatty_domain::Platform::Twitch => t!("main.warning_no_twitch_login"),
					chatty_domain::Platform::Kick => t!("main.warning_no_kick_login"),
					_ => t!("main.warning_no_login"),
				};
				col = col.push(
					container(text(warning_text.to_string()).color(palette.warning_text))
						.padding(8)
						.style(move |_theme| container::Style {
							background: Some(Background::Color(palette.warning_bg)),
							border: Border {
								radius: 4.0.into(),
								..Default::default()
							},
							..Default::default()
						}),
				);
			}
		}

		let badges_map = app.assets.get_badges_for_target(&app.state, &tab_ref.target);

		let is_focused = is_focused_at(pane, tab);

		let start_index = tab_ref.log.items.len().saturating_sub(100);
		for item in tab_ref.log.items.iter().skip(start_index) {
			match item {
				ChatItem::ChatMessage(m) => {
					let room_target = crate::ui::components::tab::TabTarget(vec![m.room.clone()]);
					let emotes_map = app.assets.get_emotes_for_target(&app.state, &room_target);
					col = col.push(
						ChatMessageView::new(app, m.as_ref(), palette, is_focused, emotes_map, badges_map.clone()).view(),
					);
				}
				ChatItem::SystemNotice(n) => {
					col = col.push(text(format!("{} {}", t!("log.system_label"), n.text)).color(palette.system_text));
				}
			}
		}

		let end_marker = container(text("")).id(format!("end-{:?}", pane));
		let col = col.push(end_marker);

		let log_id = format!("log-{:?}", pane);
		let log = scrollable(col)
			.id(log_id)
			.on_scroll(move |viewport| Message::ChatLogScrolled(pane, viewport))
			.height(Length::Fill)
			.width(Length::Fill);

		let restrictions = self.get_room_restrictions(app, &rooms);
		let placeholder = self.get_composer_placeholder(connected, &rooms, can_send, &restrictions);

		let is_composer_active = tab.focused_pane == Some(pane)
			&& app.state.ui.vim.insert_mode
			&& app.state.ui.vim.insert_target == Some(InsertTarget::Composer);

		let mut input = text_input(&placeholder, &self.composer)
			.on_input(move |v| Message::PaneMessage(pane, ChatPaneMessage::ComposerChanged(v)))
			.width(Length::Fill)
			.id(format!("composer-{:?}", pane));
		if can_compose {
			input = input.on_submit(Message::PaneMessage(pane, ChatPaneMessage::SendPressed));
		}

		let send_btn = if can_compose {
			button(text(t!("main.send_label"))).on_press(Message::PaneMessage(pane, ChatPaneMessage::SendPressed))
		} else {
			button(text(t!("main.send_label")).color(palette.text_muted))
		};

		let input_and_caret =
			container(row![input].spacing(4).align_y(Alignment::Center)).style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(if is_composer_active {
					palette.panel_bg_2
				} else {
					palette.chat_bg
				})),
				border: Border {
					color: if is_composer_active {
						palette.accent_blue
					} else {
						palette.border
					},
					width: 1.0,
					radius: 6.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			});

		let composer = row![input_and_caret, send_btn].spacing(8).align_y(Alignment::Center);

		column![log, rule::horizontal(1), composer].spacing(8).padding(8).into()
	}

	fn view_unsubscribed_pane<'a>(
		&'a self,
		app: &'a Chatty,
		pane: pane_grid::Pane,
		palette: Palette,
	) -> Element<'a, Message> {
		let connected = matches!(app.state.connection, ConnectionStatus::Connected { .. });
		let placeholder = if !connected {
			t!("main.placeholder_connect_to_send").to_string()
		} else {
			t!("main.placeholder_no_active_room").to_string()
		};

		let input = text_input(&placeholder, &self.composer)
			.on_input(move |v| Message::PaneMessage(pane, ChatPaneMessage::ComposerChanged(v)))
			.width(Length::Fill)
			.id(format!("composer-{:?}", pane));
		let send_btn = button(text(t!("main.send_label")).color(palette.text_muted));
		let composer = row![input, send_btn].spacing(8).align_y(Alignment::Center);

		let info = column![
			text(t!("main.info_join_begin")).color(palette.text_dim),
			text(t!("main.info_use_join_field")).color(palette.text_muted),
			text(t!("main.info_split_button")).color(palette.text_muted),
		]
		.spacing(8)
		.padding(12);

		let end_marker = container(text("")).id(format!("end-{:?}", pane));
		let col = info.push(end_marker);
		let log_id = format!("log-{:?}", pane);
		let log = scrollable(col)
			.id(log_id)
			.on_scroll(move |viewport| Message::ChatLogScrolled(pane, viewport))
			.height(Length::Fill)
			.width(Length::Fill);
		column![log, rule::horizontal(1), composer].spacing(8).padding(8).into()
	}

	fn get_room_restrictions(&self, app: &Chatty, rooms: &[chatty_domain::RoomKey]) -> Vec<String> {
		let mut restrictions: Vec<String> = Vec::new();
		for rk in rooms {
			if let Some(state) = app.state.room_states.get(rk) {
				if state.emote_only == Some(true) {
					restrictions.push(t!("main.room_state_emote_only").to_string());
				}
				if state.subscribers_only == Some(true) {
					restrictions.push(t!("main.room_state_subscribers_only").to_string());
				}
				if state.unique_chat == Some(true) {
					restrictions.push(t!("main.room_state_unique_chat").to_string());
				}
				if state.followers_only == Some(true) {
					let label = if let Some(minutes) = state.followers_only_duration_minutes {
						format!("{} {}m", t!("main.room_state_followers_only"), minutes)
					} else {
						t!("main.room_state_followers_only").to_string()
					};
					restrictions.push(label);
				}
				if state.slow_mode == Some(true) {
					let label = if let Some(wait) = state.slow_mode_wait_time_seconds {
						format!("{} {}s", t!("main.room_state_slow_mode"), wait)
					} else {
						t!("main.room_state_slow_mode").to_string()
					};
					restrictions.push(label);
				}
			}
		}
		restrictions.sort();
		restrictions.dedup();
		restrictions
	}

	fn get_composer_placeholder(
		&self,
		connected: bool,
		rooms: &[chatty_domain::RoomKey],
		can_send: bool,
		restrictions: &[String],
	) -> String {
		if !connected {
			t!("main.placeholder_connect_to_send").to_string()
		} else if rooms.is_empty() {
			t!("main.placeholder_no_active_room").to_string()
		} else if !can_send {
			t!("main.placeholder_no_permission").to_string()
		} else if !restrictions.is_empty() {
			format!("{} ({})", t!("main.placeholder_message"), restrictions.join(", "))
		} else {
			t!("main.placeholder_message").to_string()
		}
	}
}

fn is_focused_at(pane: pane_grid::Pane, tab: &TabModel) -> bool {
	Some(pane) == tab.focused_pane
}
