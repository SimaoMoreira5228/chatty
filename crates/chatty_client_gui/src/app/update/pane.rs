use std::str::FromStr;

use chatty_domain::RoomTopic;
use chatty_protocol::pb;
use iced::Task;
use iced::widget::{pane_grid, scrollable};
use rust_i18n::t;

use crate::app::features::chat::ChatPaneMessage;
use crate::app::message::{LayoutMessage, Message};
use crate::app::model::Chatty;
use crate::app::net::recv_next;
use crate::app::room::JoinRequest;
use crate::app::types::JoinTarget;
use crate::settings::SplitLayoutKind;

impl Chatty {
	pub fn update_layout_message(&mut self, message: LayoutMessage) -> Task<Message> {
		match message {
			LayoutMessage::ChatLogScrolled(pane, viewport) => self.update_chat_log_scrolled(pane, viewport),
			LayoutMessage::PaneClicked(pane) => self.update_pane_focus_changed(pane),
			LayoutMessage::PaneResized(ev) => self.update_pane_resized(ev),
			LayoutMessage::PaneDragged(ev) => self.update_pane_dragged(ev),
			LayoutMessage::SplitSpiral => self.update_split_spiral(),
			LayoutMessage::SplitMasonry => self.update_split_masonry(),
			LayoutMessage::SplitPressed => self.update_split_pressed(),
			LayoutMessage::CloseFocused => self.update_close_focused(),
			LayoutMessage::NavigatePaneLeft => self.update_navigate_pane_left(),
			LayoutMessage::NavigatePaneDown => self.update_navigate_pane_down(),
			LayoutMessage::NavigatePaneUp => self.update_navigate_pane_up(),
			LayoutMessage::NavigatePaneRight => self.update_navigate_pane_right(),
		}
	}

	pub fn update_chat_log_scrolled(&mut self, _pane: pane_grid::Pane, viewport: scrollable::Viewport) -> Task<Message> {
		let bounds = viewport.bounds();
		let content = viewport.content_bounds();
		let offset = viewport.absolute_offset();
		let at_end = offset.y + bounds.height + 4.0 >= content.height;
		self.state.ui.follow_end = at_end;
		Task::none()
	}

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

	pub fn update_pane_subscribed(&mut self, _pane: pane_grid::Pane, res: Result<(), String>) -> Task<Message> {
		if let Err(e) = res {
			return self.report_error(e);
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
			return self.report_error(msg);
		} else {
			// unsubscribed successfully; nothing else to do
		}

		tracing::info!("TabUnsubscribed handled; resuming network event polling");
		Task::perform(recv_next(self.net_rx.clone()), |ev| {
			Message::Net(crate::app::message::NetMessage::NetPolled(ev))
		})
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

		let net = self.net_effects.clone();
		Task::perform(async move { net.send_command(cmd).await.map_err(|e: String| e) }, |res| {
			Message::Chat(crate::app::message::ChatMessage::Sent(res))
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
		self.update_open_join_modal(JoinTarget::Split)
	}

	pub fn update_open_join_modal(&mut self, target: JoinTarget) -> Task<Message> {
		self.state.ui.pending_join_target = Some(target);
		self.state.ui.active_overlay = Some(crate::app::features::overlays::ActiveOverlay::Join(
			crate::app::features::overlays::JoinModal::new(self.state.default_platform),
		));
		Task::none()
	}

	pub fn update_join_modal_submit(&mut self, modal: crate::app::features::overlays::JoinModal) -> Task<Message> {
		let raw_input = modal.input.trim().to_string();
		if raw_input.is_empty() {
			return Task::none();
		}

		let platform = modal.platform;
		let raw = raw_input
			.split(',')
			.map(|part| {
				let part = part.trim();
				if part.is_empty() {
					return String::new();
				}
				if part.starts_with(RoomTopic::PREFIX) || part.contains(':') {
					part.to_string()
				} else {
					format!("{}:{}", platform.as_str(), part)
				}
			})
			.filter(|p| !p.is_empty())
			.collect::<Vec<_>>()
			.join(", ");

		let req = JoinRequest { raw };
		let rooms = self.state.parse_join_rooms(&req);
		if rooms.is_empty() {
			return self.toast(t!("invalid_room").to_string());
		}

		self.state.ui.active_overlay = None;
		self.state.ui.overlay_dismissed = true;

		let join_target = self.state.ui.pending_join_target.take().unwrap_or(JoinTarget::Split);
		let join_raw = rooms
			.iter()
			.map(|r| format!("{}:{}", r.platform.as_str(), r.room_id.as_str()))
			.collect::<Vec<_>>()
			.join(", ");

		let has_selected_tab = self.selected_tab_id().is_some();
		match join_target {
			JoinTarget::NewTab | JoinTarget::Split if !has_selected_tab => {
				let title = rooms.iter().map(|r| r.room_id.as_str()).collect::<Vec<_>>().join(", ");
				let tid = self.state.create_tab_for_rooms(title, rooms.clone());
				self.state.selected_tab_id = Some(tid);
			}
			JoinTarget::Split => {
				let mut sorted_rooms = rooms.clone();
				sorted_rooms.sort_by(|a, b| {
					a.platform
						.as_str()
						.cmp(b.platform.as_str())
						.then_with(|| a.room_id.as_str().cmp(b.room_id.as_str()))
				});

				let existing_tid = self.state.tabs.iter().find_map(|(tid, t)| {
					let mut t_rooms = t.target.0.clone();
					t_rooms.sort_by(|a, b| {
						a.platform
							.as_str()
							.cmp(b.platform.as_str())
							.then_with(|| a.room_id.as_str().cmp(b.room_id.as_str()))
					});
					(t_rooms == sorted_rooms).then_some(*tid)
				});

				let (tid, created_new) = match existing_tid {
					Some(tid) => (tid, false),
					None => {
						let title = rooms.iter().map(|r| r.room_id.as_str()).collect::<Vec<_>>().join(", ");
						let tid = self.state.create_tab_for_rooms(title, rooms.clone());
						(tid, true)
					}
				};

				if created_new {
					self.state.tab_order.retain(|&id| id != tid);
				}

				let split_layout = self.state.gui_settings().split_layout;
				let window_size = self.state.ui.window_size;
				let mut spiral_dir = self.state.ui.spiral_dir;
				let mut masonry_flip = self.state.ui.masonry_flip;

				if let Some(tab) = self.selected_tab_mut() {
					if tab.panes.iter().next().is_none() {
						let (panes, pane) = pane_grid::State::new(crate::app::features::chat::ChatPane::new(Some(tid)));
						tab.panes = panes;
						tab.focused_pane = Some(pane);
						if let Some(p) = tab.panes.get_mut(pane) {
							p.join_raw = join_raw;
						}
					} else {
						let Some(focused) = tab.focused_pane.or_else(|| tab.panes.iter().next().map(|(id, _)| *id)) else {
							return Task::none();
						};

						let mut new_state = crate::app::features::chat::ChatPane::new(Some(tid));
						new_state.join_raw = join_raw;

						match split_layout {
							SplitLayoutKind::Linear => {
								let axis = pane_grid::Axis::Vertical;
								let ratio = 0.5;
								if let Some((new_pane, split)) = tab.panes.split(axis, focused, new_state) {
									tab.panes.resize(split, ratio);
									tab.focused_pane = Some(new_pane);
								}
							}
							SplitLayoutKind::Spiral => {
								let dir = spiral_dir % 4;
								spiral_dir = (spiral_dir + 1) % 4;
								let (axis, ratio, swap) = match dir {
									0 => (pane_grid::Axis::Vertical, 0.618, false),
									1 => (pane_grid::Axis::Horizontal, 0.618, false),
									2 => (pane_grid::Axis::Vertical, 0.618, true),
									3 => (pane_grid::Axis::Horizontal, 0.618, true),
									_ => unreachable!(),
								};
								if let Some((new_pane, split)) = tab.panes.split(axis, focused, new_state) {
									tab.panes.resize(split, ratio);
									if swap {
										tab.panes.swap(focused, new_pane);
									}
									tab.focused_pane = Some(focused);
								}
							}
							SplitLayoutKind::Masonry => {
								let Some((width, height)) = window_size else {
									return Task::none();
								};
								let bounds = iced::Size::new(width, height);

								let mut best: Option<(pane_grid::Pane, iced::Rectangle, f32)> = None;
								for (pane, rect) in tab.panes.layout().pane_regions(8.0, 50.0, bounds) {
									let area = rect.width * rect.height;
									best = match best {
										None => Some((pane, rect, area)),
										Some((bp, br, ba)) => {
											let is_focused = Some(pane) == tab.focused_pane;
											if area > ba || (area == ba && is_focused) {
												Some((pane, rect, area))
											} else {
												Some((bp, br, ba))
											}
										}
									};
								}

								let Some((target_pane, rect, _)) = best else {
									return Task::none();
								};

								let axis = if rect.width >= rect.height {
									pane_grid::Axis::Vertical
								} else {
									pane_grid::Axis::Horizontal
								};

								let flip = masonry_flip;
								masonry_flip = !masonry_flip;
								let ratio = if flip { 0.5 } else { 0.618 };
								let swap = flip;

								if let Some((new_pane, split)) = tab.panes.split(axis, target_pane, new_state) {
									tab.panes.resize(split, ratio);
									if swap {
										tab.panes.swap(target_pane, new_pane);
									}
									tab.focused_pane = Some(target_pane);
								}
							}
						}
					}
				}
				self.state.ui.spiral_dir = spiral_dir;
				self.state.ui.masonry_flip = masonry_flip;
			}
			JoinTarget::NewTab => {
				let title = rooms.iter().map(|r| r.room_id.as_str()).collect::<Vec<_>>().join(", ");
				let tid = self.state.create_tab_for_rooms(title, rooms.clone());
				self.state.selected_tab_id = Some(tid);
			}
		}

		self.save_ui_layout();

		let mut unique_rooms: std::collections::HashSet<chatty_domain::RoomKey> = std::collections::HashSet::new();
		for tab in self.state.tabs.values() {
			for room in &tab.target.0 {
				unique_rooms.insert(room.clone());
			}
		}

		let net = self.net_effects.clone();
		Task::perform(
			async move {
				let mut results = Vec::new();
				for room in unique_rooms.into_iter() {
					let res = net.subscribe_room_key(room.clone()).await;
					results.push((room, res));
				}
				results
			},
			|results| Message::Net(crate::app::message::NetMessage::AutoJoinCompleted(results)),
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
			let net = self.net_effects.clone();
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
		let mut toaster = std::mem::replace(&mut self.state.ui.toaster, crate::app::features::toaster::Toaster::new());
		let task = toaster.update(self, crate::app::features::toaster::ToasterMessage::Dismiss);
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
