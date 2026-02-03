use chatty_domain::RoomTopic;
use iced::{Task, keyboard};
use rust_i18n::t;

use crate::app::message::Message;
use crate::app::model::{Chatty, first_char_lower};
use crate::app::net::recv_next;
use crate::app::state::ConnectionStatus;
use crate::app::subscription::shortcut_match;
use crate::app::types::{InsertTarget, JoinTarget};
use crate::settings;

impl Chatty {
	pub fn update_char_pressed(&mut self, ch: char, modifiers: keyboard::Modifiers) -> Task<Message> {
		let k = settings::get_cloned().keybinds;
		let Some(tab) = self.selected_tab_mut() else {
			return Task::none();
		};

		if shortcut_match(modifiers, k.drag_modifier) {
			if first_char_lower(&k.close_key) == ch {
				return self.update_delete_pressed();
			}

			if first_char_lower(&k.new_key) == ch {
				return self.update_open_join_modal(JoinTarget::Split);
			}

			if first_char_lower(&k.reconnect_key) == ch {
				let mut gs = self.state.gui_settings().clone();
				if !chatty_client_core::ClientConfigV1::server_endpoint_locked() {
					gs.server_endpoint_quic = self.state.ui.server_endpoint_quic.clone();
				}
				gs.server_auth_token = self.state.ui.server_auth_token.clone();

				let cfg = match settings::build_client_config(&gs) {
					Ok(c) => c,
					Err(e) => {
						return self.report_error(e);
					}
				};

				self.state.set_connection_status(ConnectionStatus::Connecting);
				let net = self.net_effects.clone();
				return Task::perform(net.connect(cfg), |res| {
					Message::Net(crate::app::message::NetMessage::ConnectFinished(res))
				});
			}
		}

		if k.vim_nav {
			if k.vim_left_key.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0') == ch {
				self.navigate_pane(-1, 0);
				self.save_ui_layout();
				return Task::none();
			}
			if k.vim_down_key.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0') == ch {
				self.navigate_pane(0, 1);
				self.save_ui_layout();
				return Task::none();
			}
			if k.vim_up_key.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0') == ch {
				self.navigate_pane(0, -1);
				self.save_ui_layout();
				return Task::none();
			}
			if k.vim_right_key.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0') == ch {
				self.navigate_pane(1, 0);
				self.save_ui_layout();
				return Task::none();
			}
		}

		if k.vim_nav && modifiers == iced::keyboard::Modifiers::default() && ch == 'i' {
			let Some(focused) = tab.focused_pane.or_else(|| tab.panes.iter().next().map(|(id, _)| *id)) else {
				return Task::none();
			};

			let mut use_composer = false;
			if let Some(ps) = tab.panes.get(focused)
				&& let Some(tid) = ps.tab_id
				&& let Some(t) = self.state.tabs.get(&tid)
			{
				let rooms = t.target.0.clone();
				let can_send = rooms
					.iter()
					.any(|rk| self.state.room_permissions.get(rk).map(|p| p.can_send).unwrap_or(true));
				let connected = matches!(self.state.connection, crate::app::state::ConnectionStatus::Connected { .. });
				use_composer = connected && !rooms.is_empty() && can_send;
			}
			if use_composer {
				self.state.ui.vim.enter_insert_mode(InsertTarget::Composer);
				let toast_cmd = self.toast(t!("insert_mode").to_string());
				let focus_cmd = iced::widget::operation::focus(format!("composer-{:?}", focused));
				let recv_cmd = Task::perform(recv_next(self.net_rx.clone()), |ev| {
					Message::Net(crate::app::message::NetMessage::NetPolled(ev))
				});
				return Task::batch(vec![toast_cmd, focus_cmd, recv_cmd]);
			}

			return self.update_open_join_modal(JoinTarget::Split);
		}

		Task::none()
	}

	pub fn update_delete_pressed(&mut self) -> Task<Message> {
		let focused = self.selected_tab().and_then(|t| t.focused_pane);
		let Some(focused) = focused else {
			return Task::none();
		};

		let room_opt = self.pane_room(focused);
		let mut tid_to_remove = None;

		if let Some(tab) = self.selected_tab_mut() {
			let tab_id_opt = tab.panes.get(focused).and_then(|p| p.tab_id);

			if let Some((_closed, sibling)) = tab.panes.close(focused) {
				tab.focused_pane = Some(sibling);

				if let (Some(tid), Some(room)) = (tab_id_opt, room_opt) {
					let still_referenced = tab.panes.iter().any(|(_, p)| p.tab_id == Some(tid));
					if !still_referenced {
						tid_to_remove = Some((tid, room));
					}
				}
			}
		} else {
			return Task::none();
		}

		self.save_ui_layout();

		if let Some((tid, room)) = tid_to_remove {
			self.state.tabs.remove(&tid);
			let net = self.net_effects.clone();
			return Task::perform(async move { net.unsubscribe_room_key(room).await }, |res| match res {
				Ok(_) => Message::DismissToast,
				Err(e) => {
					eprintln!("Failed to unsubscribe from room: {}", e);
					Message::DismissToast
				}
			});
		}

		Task::none()
	}

	pub fn update_named_key_pressed(&mut self, named: iced::keyboard::key::Named) -> Task<Message> {
		use iced::keyboard::key::Named;
		match named {
			Named::Escape => {
				if let Some(crate::app::features::overlays::ActiveOverlay::MessageAction(_)) = &self.state.ui.active_overlay
				{
					self.state.ui.active_overlay = None;
					return Task::none();
				}
				if self.state.ui.active_overlay.is_some() {
					return self.update_modal_dismissed();
				} else if self.state.ui.vim.insert_mode {
					self.state.ui.vim.exit_insert_mode();
					let toast_cmd = self.toast(t!("insert_mode_exited").to_string());

					let unfocus_cmd =
						iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus());
					let recv_cmd = Task::perform(recv_next(self.net_rx.clone()), |ev| {
						Message::Net(crate::app::message::NetMessage::NetPolled(ev))
					});
					return Task::batch(vec![toast_cmd, unfocus_cmd, recv_cmd]);
				}
				Task::none()
			}
			Named::Enter => {
				if self.state.ui.vim.insert_mode {
					let focused = self.selected_tab().and_then(|t| t.focused_pane);
					let Some(focused) = focused else {
						return Task::none();
					};
					let rooms = self.selected_tab().map(|t| t.target.0.clone()).unwrap_or_default();
					let insert_target = self.state.ui.vim.insert_target;

					if let Some(tab) = self.selected_tab_mut()
						&& let Some(p) = tab.panes.get_mut(focused)
					{
						match insert_target {
							Some(InsertTarget::Composer) => {
								if !p.composer.is_empty() && !rooms.is_empty() {
									let text = p.composer.trim().to_string();
									if text.is_empty() {
										return Task::none();
									}
									let reply_to_server_message_id = p.reply_to_server_message_id.clone();
									let reply_to_platform_message_id = p.reply_to_platform_message_id.clone();
									p.composer.clear();
									self.state.ui.vim.exit_insert_mode();

									self.save_ui_layout();

									let room = rooms[0].clone();
									let topic = RoomTopic::format(&room);
									let cmd = chatty_protocol::pb::Command {
										command: Some(chatty_protocol::pb::command::Command::SendChat(
											chatty_protocol::pb::SendChatCommand {
												topic,
												text,
												reply_to_server_message_id,
												reply_to_platform_message_id,
											},
										)),
									};

									let net = self.net_effects.clone();
									return Task::perform(
										async move { net.send_command(cmd).await.map_err(|e| e.to_string()) },
										|res| Message::Chat(crate::app::message::ChatMessage::Sent(res)),
									);
								}
							}
							None => {}
						}
					}

					Task::none()
				} else {
					Task::none()
				}
			}
			Named::Backspace => {
				if let Some(crate::app::features::overlays::ActiveOverlay::MessageAction(_)) = &self.state.ui.active_overlay
				{
					let focused = self.selected_tab().and_then(|t| t.focused_pane);
					let Some(focused) = focused else {
						return Task::none();
					};

					let insert_target = self.state.ui.vim.insert_target;

					if let Some(tab) = self.selected_tab_mut() {
						if let Some(p) = tab.panes.get_mut(focused)
							&& let Some(InsertTarget::Composer) = insert_target
						{
							p.composer.pop();
						}
						self.save_ui_layout();
					}
				}
				Task::none()
			}
			_ => Task::none(),
		}
	}

	pub fn update_message_text_edit(&mut self, key: String, action: iced::widget::text_editor::Action) -> Task<Message> {
		if let Some(content) = self.message_text_editors.get_mut(&key)
			&& !action.is_edit()
		{
			content.perform(action);
		}
		Task::none()
	}
}
