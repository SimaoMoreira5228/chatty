#![forbid(unsafe_code)]

use chatty_client_ui::app_state::{ChatItem, ConnectionStatus, TabTarget};
use iced::widget::pane_grid;
use iced::widget::pane_grid::PaneGrid;
use iced::widget::{button, column, container, image, row, rule, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow};

use crate::app::{Chatty, InsertTarget, Message, PaneState, PendingCommand};
use crate::theme;

fn join_row(
	pane: pane_grid::Pane,
	pane_state: &PaneState,
	is_active: bool,
	palette: theme::Palette,
) -> Element<'static, Message> {
	let join_placeholder = t!("main.join_placeholder").to_string();
	let mut join = text_input(&join_placeholder, &pane_state.join_raw)
		.on_input(move |v| Message::PaneJoinChanged(pane, v))
		.width(Length::Fill)
		.id(format!("join-{:?}", pane));
	join = join.on_submit(Message::PaneJoinPressed(pane));

	let input = row![join].spacing(4).align_y(Alignment::Center);
	let input = container(input).style(move |_theme| container::Style {
		text_color: Some(palette.text),
		background: Some(Background::Color(if is_active {
			palette.panel_bg_2
		} else {
			palette.chat_bg
		})),
		border: Border {
			color: if is_active { palette.accent_blue } else { palette.border },
			width: 1.0,
			radius: 6.0.into(),
		},
		shadow: Shadow::default(),
		snap: false,
	});

	row![
		input,
		button(text(t!("main.join_button"))).on_press(Message::PaneJoinPressed(pane))
	]
	.spacing(8)
	.align_y(Alignment::Center)
	.into()
}

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let panes = {
		let view = |pane: pane_grid::Pane, pane_state: &PaneState, _is_focused: bool| {
			let is_focused = pane == app.focused_pane;
			let title = pane_state
				.tab_id
				.and_then(|tid| app.state.tabs.get(&tid).map(|t| t.title.clone()))
				.unwrap_or_else(|| t!("main.welcome").to_string());

			let title_color = if is_focused { palette.text } else { palette.text_dim };
			let title_bar = pane_grid::TitleBar::new(text(title).color(title_color)).padding(6);

			let body: Element<'_, Message> = match pane_state.tab_id.and_then(|tid| app.state.tabs.get(&tid)) {
				Some(tab) => {
					let room_key = match &tab.target {
						TabTarget::Room(room) => Some(room.clone()),
						_ => None,
					};
					let can_send = room_key
						.as_ref()
						.and_then(|rk| app.state.room_permissions.get(rk))
						.map(|p| p.can_send)
						.unwrap_or(true);
					let connected = matches!(app.state.connection, ConnectionStatus::Connected { .. });
					let can_compose = connected && room_key.is_some() && can_send;

					let mut col = column![].spacing(4);
					for item in tab.log.items.iter().rev().take(300).rev() {
						match item {
							ChatItem::ChatMessage(m) => {
								let name = m.user_display.clone().unwrap_or_else(|| m.user_login.clone());

								let mut msg_row = row![].spacing(6).align_y(Alignment::Center);
								if !m.badge_ids.is_empty()
									&& let Some(keys) = app.state.room_asset_cache_keys.get(&m.room)
								{
									for cache_key in keys {
										if let Some(bundle) = app.state.asset_bundles.get(cache_key) {
											for badge in &bundle.badges {
												if m.badge_ids.iter().any(|id| id == &badge.id) {
													let mut icache = app.image_cache.lock().unwrap();
													if let Some(handle) = icache.get(&badge.image_url) {
														msg_row = msg_row.push(image(handle.clone()).width(18).height(18));
													} else {
														let loading =
															app.image_loading.lock().unwrap().contains(&badge.image_url);
														let failed =
															app.image_failed.lock().unwrap().contains(&badge.image_url);
														drop(icache);
														if loading {
															msg_row = msg_row.push(text("◌").color(palette.text_muted));
														} else if failed {
															msg_row = msg_row.push(text("[x]").color(palette.system_text));
														} else {
															let _ = app.image_fetch_sender.try_send(badge.image_url.clone());
															msg_row = msg_row.push(
																text(format!("[{}]", badge.name)).color(palette.text_dim),
															);
														}
													}
												}
											}
										}
									}
								}

								let display_name = format!("{}: {}", name, m.text);

								let action_btn = button(text("⋯")).on_press(Message::MessageActionButtonPressed(
									m.room.clone(),
									m.server_message_id.clone(),
									m.platform_message_id.clone(),
									m.author_id.clone(),
								));
								msg_row = msg_row.push(text(display_name).color(if is_focused {
									palette.text
								} else {
									palette.text_dim
								}));
								msg_row = msg_row.push(action_btn);
								let is_pending = app.pending_commands.iter().any(|pc| match pc {
									PendingCommand::Delete {
										room: r,
										server_message_id: s,
										platform_message_id: p,
									} => {
										(r == &m.room)
											&& s.as_ref() == m.server_message_id.as_ref()
											&& p.as_ref() == m.platform_message_id.as_ref()
									}
									_ => false,
								});
								if is_pending {
									msg_row = msg_row.push(text(" ⏳").color(palette.text_muted));
								}

								if let Some(active) = &app.pending_message_action {
									if active.0 == m.room
										&& active.1 == m.server_message_id
										&& active.2 == m.platform_message_id
									{
										let mut actions = row![].spacing(6).align_y(Alignment::Center);
										actions = actions.push(button(text(t!("actions.reply"))).on_press(
											Message::ReplyToMessage(
												m.room.clone(),
												m.server_message_id.clone(),
												m.platform_message_id.clone(),
											),
										));

										if let Some(perms) = app.state.room_permissions.get(&m.room) {
											if perms.can_delete {
												actions = actions.push(button(text(t!("actions.delete"))).on_press(
													Message::DeleteMessage(
														m.room.clone(),
														m.server_message_id.clone(),
														m.platform_message_id.clone(),
													),
												));
											}
											if perms.can_timeout
												&& let Some(uid) = &m.author_id
											{
												actions = actions.push(
													button(text(t!("actions.timeout")))
														.on_press(Message::TimeoutUser(m.room.clone(), uid.clone())),
												);
											}
											if perms.can_ban
												&& let Some(uid) = &m.author_id
											{
												actions = actions.push(
													button(text(t!("actions.ban")))
														.on_press(Message::BanUser(m.room.clone(), uid.clone())),
												);
											}
										}
										col = col.push(actions);
									} else {
										col = col.push(msg_row);
									}
								} else {
									col = col.push(msg_row);
								}
							}
							ChatItem::SystemNotice(n) => {
								col = col
									.push(text(format!("{} {}", t!("log.system_label"), n.text)).color(palette.system_text));
							}
							ChatItem::Lagged(l) => {
								col = col.push(
									text(format!(
										"{} dropped={} {}",
										t!("log.lagged_label"),
										l.dropped,
										l.detail.clone().unwrap_or_default()
									))
									.color(palette.system_text),
								);
							}
						}
					}

					let log = scrollable(col).height(Length::Fill).width(Length::Fill);

					let is_join_active =
						app.insert_mode && app.insert_target == Some(InsertTarget::Join) && pane == app.focused_pane;
					let join_row = join_row(pane, pane_state, is_join_active, palette);

					let placeholder = if !connected {
						t!("main.placeholder_connect_to_send").to_string()
					} else if room_key.is_none() {
						t!("main.placeholder_no_active_room").to_string()
					} else if !can_send {
						t!("main.placeholder_no_permission").to_string()
					} else {
						t!("main.placeholder_message").to_string()
					};

					let is_composer_active =
						app.insert_mode && app.insert_target == Some(InsertTarget::Composer) && pane == app.focused_pane;
					let mut input = text_input(&placeholder, &pane_state.composer)
						.on_input(move |v| Message::PaneComposerChanged(pane, v))
						.width(Length::Fill)
						.id(format!("composer-{:?}", pane));
					if can_compose {
						input = input.on_submit(Message::PaneSendPressed(pane));
					}

					let send_btn = if can_compose {
						button(text(t!("main.send_label"))).on_press(Message::PaneSendPressed(pane))
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

					column![log, rule::horizontal(1), join_row, composer]
						.spacing(8)
						.padding(8)
						.into()
				}
				None => {
					let connected = matches!(app.state.connection, ConnectionStatus::Connected { .. });
					let is_join_active =
						app.insert_mode && app.insert_target == Some(InsertTarget::Join) && pane == app.focused_pane;
					let join_row = join_row(pane, pane_state, is_join_active, palette);
					let placeholder = if !connected {
						t!("main.placeholder_connect_to_send").to_string()
					} else {
						t!("main.placeholder_no_active_room").to_string()
					};

					let input = text_input(&placeholder, &pane_state.composer)
						.on_input(move |v| Message::PaneComposerChanged(pane, v))
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

					let log = scrollable(info).height(Length::Fill).width(Length::Fill);
					column![log, rule::horizontal(1), join_row, composer]
						.spacing(8)
						.padding(8)
						.into()
				}
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
		};

		let mut grid = PaneGrid::new(&app.panes, view)
			.width(Length::Fill)
			.height(Length::Fill)
			.spacing(8)
			.on_click(Message::PaneClicked)
			.on_resize(10, Message::PaneResized);

		if app.pane_drag_enabled() {
			grid = grid.on_drag(Message::PaneDragged);
		}

		grid
	};

	container(panes).width(Length::Fill).height(Length::Fill).padding(12).into()
}
