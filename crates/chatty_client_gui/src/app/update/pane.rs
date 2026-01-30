use std::str::FromStr;

use chatty_domain::RoomTopic;
use chatty_protocol::pb;
use iced::Task;
use iced::widget::pane_grid;
use rust_i18n::t;

use crate::app::net::recv_next;
use crate::app::state::JoinRequest;
use crate::app::{Chatty, Message};
use crate::settings::SplitLayoutKind;
use crate::ui::components::chat_pane::ChatPaneMessage;

impl Chatty {
	pub fn update_pane_message(&mut self, pane: pane_grid::Pane, msg: ChatPaneMessage) -> Task<Message> {
		if let Some(tab) = self.selected_tab_mut()
			&& let Some(mut p) = tab.panes.get_mut(pane).cloned()
		{
			let task = p.update(pane, msg, self);
			if let Some(tab) = self.selected_tab_mut()
				&& let Some(pane_ref) = tab.panes.get_mut(pane)
			{
				*pane_ref = p;
			}
			return task;
		}
		Task::none()
	}

	pub fn update_pane_join_pressed(&mut self, pane: pane_grid::Pane) -> Task<Message> {
		let raw = self
			.selected_tab()
			.and_then(|t| t.panes.get(pane))
			.map(|p| p.join_raw.clone())
			.unwrap_or_default();
		let req = JoinRequest { raw };
		let rooms = self.state.parse_join_rooms(&req);
		if rooms.is_empty() {
			self.state
				.push_notification(crate::app::state::UiNotificationKind::Warning, t!("invalid_room").to_string());
			return Task::none();
		};

		let tid = self.ensure_tab_for_rooms(rooms.clone());
		if let Some(tab) = self.selected_tab_mut()
			&& let Some(p) = tab.panes.get_mut(pane)
		{
			p.tab_id = Some(tid);
			p.join_raw = rooms
				.iter()
				.map(|r| format!("{}:{}", r.platform.as_str(), r.room_id.as_str()))
				.collect::<Vec<_>>()
				.join(", ");
		}

		let net = self.net.clone();
		Task::perform(
			async move {
				let mut results = Vec::new();
				for room in rooms {
					let res = net.subscribe_room_key(room.clone()).await;
					results.push((room, res));
				}
				results
			},
			Message::AutoJoinCompleted,
		)
	}

	pub fn update_pane_subscribed(&mut self, _pane: pane_grid::Pane, res: Result<(), String>) -> Task<Message> {
		if let Err(e) = res {
			let t = self.toast(e.clone());
			self.state.push_notification(crate::app::state::UiNotificationKind::Error, e);
			return t;
		} else {
			self.save_ui_layout();
		}

		Task::none()
	}

	pub fn update_tab_unsubscribed(&mut self, room: chatty_domain::RoomKey, res: Result<(), String>) -> Task<Message> {
		if let Err(e) = res {
			let msg = format!(
				"{} {}: {}",
				t!("failed_to_unsubscribe"),
				chatty_domain::RoomTopic::format(&room),
				e
			);
			let t = self.toast(msg.clone());
			self.state
				.push_notification(crate::app::state::UiNotificationKind::Error, msg);
			return t;
		} else {
			// unsubscribed successfully; nothing else to do
		}

		tracing::info!("TabUnsubscribed handled; resuming network event polling");
		Task::perform(recv_next(self.net_rx.clone()), Message::NetPolled)
	}

	pub fn update_pane_send_pressed(&mut self, pane: pane_grid::Pane) -> Task<Message> {
		let rooms = self.pane_rooms(pane);
		if rooms.is_empty() {
			return self.toast(t!("no_active_room").to_string());
		}

		let (text, reply_to_server_message_id, reply_to_platform_message_id, reply_to_room) =
			if let Some(tab) = self.selected_tab() {
				if let Some(p) = tab.panes.get(pane) {
					(
						p.composer.trim().to_string(),
						p.reply_to_server_message_id.clone(),
						p.reply_to_platform_message_id.clone(),
						p.reply_to_room.clone(),
					)
				} else {
					return Task::none();
				}
			} else {
				return Task::none();
			};

		if text.is_empty() {
			return Task::none();
		}

		let room = if let Some(r) = reply_to_room {
			r
		} else {
			let mut r = rooms[0].clone();
			if text.starts_with('/') {
				let parts: Vec<&str> = text.split_whitespace().collect();
				if parts.len() >= 2 {
					let platform_hint = parts[1].to_lowercase();
					if let Ok(p) = chatty_domain::Platform::from_str(&platform_hint)
						&& let Some(target) = rooms.iter().find(|tr| tr.platform == p)
					{
						r = target.clone();
					}
				}
			}
			r
		};

		if let Some(tab) = self.selected_tab_mut()
			&& let Some(p) = tab.panes.get_mut(pane)
		{
			p.composer.clear();
			p.reply_to_server_message_id.clear();
			p.reply_to_platform_message_id.clear();
			p.reply_to_room = None;
		}

		self.save_ui_layout();

		let topic = RoomTopic::format(&room);
		let cmd = pb::Command {
			command: Some(pb::command::Command::SendChat(pb::SendChatCommand {
				topic,
				text,
				reply_to_server_message_id,
				reply_to_platform_message_id,
			})),
		};

		let net = self.net.clone();
		Task::perform(async move { net.send_command(cmd).await.map_err(|e: String| e) }, |res| {
			Message::Sent(res)
		})
	}

	pub fn update_pane_focus_changed(&mut self, pane: pane_grid::Pane) -> Task<Message> {
		if let Some(tab) = self.selected_tab_mut() {
			tab.focused_pane = Some(pane);
		}
		self.save_ui_layout();
		Task::none()
	}

	pub fn update_pane_resized(&mut self, ev: pane_grid::ResizeEvent) -> Task<Message> {
		if let Some(tab) = self.selected_tab_mut() {
			tab.panes.resize(ev.split, ev.ratio);
		}
		self.save_ui_layout();
		Task::none()
	}

	pub fn update_pane_dragged(&mut self, ev: pane_grid::DragEvent) -> Task<Message> {
		if let Some(tab) = self.selected_tab_mut() {
			match ev {
				pane_grid::DragEvent::Dropped { pane, target } => {
					tab.panes.drop(pane, target);
					tab.focused_pane = Some(pane);
					self.save_ui_layout();
				}
				pane_grid::DragEvent::Picked { pane } => {
					tab.focused_pane = Some(pane);
				}
				_ => {}
			}
		}
		Task::none()
	}

	pub fn update_split_spiral(&mut self) -> Task<Message> {
		self.split_spiral();
		self.save_ui_layout();
		Task::none()
	}

	pub fn update_split_masonry(&mut self) -> Task<Message> {
		self.split_masonry();
		self.save_ui_layout();
		Task::none()
	}

	pub fn update_split_pressed(&mut self) -> Task<Message> {
		let split_layout = self.state.gui_settings().split_layout;
		let has_panes = self.selected_tab().map(|t| t.panes.iter().count() > 0).unwrap_or(false);

		if !has_panes {
			return self.update_open_join_modal();
		}

		match split_layout {
			SplitLayoutKind::Spiral => self.split_spiral(),
			SplitLayoutKind::Linear => self.split_linear(),
			SplitLayoutKind::Masonry => self.split_masonry(),
		}

		self.save_ui_layout();
		Task::none()
	}

	pub fn update_open_join_modal(&mut self) -> Task<Message> {
		self.state.ui.active_overlay = Some(crate::ui::modals::ActiveOverlay::Join(crate::ui::modals::JoinModal::new(
			self.state.default_platform,
		)));
		Task::none()
	}

	pub fn update_join_modal_submit(&mut self) -> Task<Message> {
		let Some(crate::ui::modals::ActiveOverlay::Join(m)) = self.state.ui.active_overlay.take() else {
			return Task::none();
		};

		let raw = m.input.trim().to_string();
		if raw.is_empty() {
			return Task::none();
		}

		let req = JoinRequest { raw };
		let rooms = self.state.parse_join_rooms(&req);
		if rooms.is_empty() {
			return self.toast(t!("invalid_room").to_string());
		}

		let tid = self.ensure_tab_for_rooms(rooms.clone());
		let split_layout = self.state.gui_settings().split_layout;
		let join_raw = rooms
			.iter()
			.map(|r| format!("{}:{}", r.platform.as_str(), r.room_id.as_str()))
			.collect::<Vec<_>>()
			.join(", ");

		if let Some(tab) = self.selected_tab_mut() {
			if tab.panes.iter().next().is_none() {
				let (panes, pane) = pane_grid::State::new(crate::ui::components::chat_pane::ChatPane::new(Some(tid)));
				tab.panes = panes;
				tab.focused_pane = Some(pane);
				// Set join_raw
				if let Some(p) = tab.panes.get_mut(pane) {
					p.join_raw = join_raw;
				}
			} else {
				let axis = match split_layout {
					SplitLayoutKind::Linear => pane_grid::Axis::Vertical,
					_ => {
						if tab.panes.iter().count() % 2 == 0 {
							pane_grid::Axis::Horizontal
						} else {
							pane_grid::Axis::Vertical
						}
					}
				};

				let Some(focused) = tab.focused_pane.or_else(|| tab.panes.iter().next().map(|(id, _)| *id)) else {
					return Task::none();
				};

				let mut new_state = crate::ui::components::chat_pane::ChatPane::new(Some(tid));
				new_state.join_raw = join_raw;

				if let Some((new_pane, _)) = tab.panes.split(axis, focused, new_state) {
					tab.focused_pane = Some(new_pane);
				}
			}
		}

		self.save_ui_layout();

		let net = self.net.clone();
		Task::perform(
			async move {
				let mut results = Vec::new();
				for room in rooms {
					let res = net.subscribe_room_key(room.clone()).await;
					results.push((room, res));
				}
				results
			},
			Message::AutoJoinCompleted,
		)
	}

	pub fn update_close_focused(&mut self) -> Task<Message> {
		let focused = self.selected_tab().and_then(|t| t.focused_pane);
		let Some(focused) = focused else {
			return Task::none();
		};

		let mut tid_to_remove = None;
		let rooms = self.pane_rooms(focused);

		if let Some(tab) = self.selected_tab_mut() {
			let tab_id_opt = tab.panes.get(focused).and_then(|p| p.tab_id);

			if let Some((_closed, sibling)) = tab.panes.close(focused) {
				tab.focused_pane = Some(sibling);

				if let Some(tid) = tab_id_opt {
					let still_referenced = tab.panes.iter().any(|(_, p)| p.tab_id == Some(tid));
					if !still_referenced {
						tid_to_remove = Some(tid);
					}
				}
			}
		} else {
			return Task::none();
		}

		self.save_ui_layout();

		if let Some(tid) = tid_to_remove {
			self.state.tabs.remove(&tid);
			let net = self.net.clone();
			return Task::perform(
				async move {
					let mut results = Vec::new();
					for room in rooms {
						let res = net.unsubscribe_room_key(room.clone()).await;
						results.push((room, res));
					}
					results
				},
				move |results| {
					if let Some((_room, _res)) = results.into_iter().next() {
						Message::DismissToast
					} else {
						Message::DismissToast
					}
				},
			);
		}

		Task::none()
	}

	pub fn update_dismiss_toast(&mut self) -> Task<Message> {
		let mut toaster = std::mem::replace(&mut self.state.ui.toaster, crate::ui::components::toaster::Toaster::new());
		let task = toaster.update(self, crate::ui::components::toaster::ToasterMessage::Dismiss);
		self.state.ui.toaster = toaster;
		task
	}

	pub fn update_navigate_pane_left(&mut self) -> Task<Message> {
		self.navigate_pane(-1, 0);
		Task::none()
	}

	pub fn update_navigate_pane_down(&mut self) -> Task<Message> {
		self.navigate_pane(0, 1);
		Task::none()
	}

	pub fn update_navigate_pane_up(&mut self) -> Task<Message> {
		self.navigate_pane(0, -1);
		Task::none()
	}

	pub fn update_navigate_pane_right(&mut self) -> Task<Message> {
		self.navigate_pane(1, 0);
		Task::none()
	}
}
