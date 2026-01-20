#![forbid(unsafe_code)]

use std::time::SystemTime;

use chatty_client_ui::app_state::{ConnectionStatus, JoinRequest, UiNotificationKind};
use chatty_client_ui::net::UiEvent;
use chatty_client_ui::settings::{self, SplitLayoutKind};
use chatty_domain::RoomTopic;
use chatty_protocol::pb;
use iced::Task;
use iced::clipboard;
use iced::widget::pane_grid;

use crate::app::model::{InsertTarget, first_char_lower};
use crate::app::net::recv_next;
use crate::app::subscription::shortcut_match;
use crate::app::{Chatty, ClipboardTarget, Message};
use chatty_domain::Platform;

impl Chatty {
	pub fn update(&mut self, message: Message) -> Task<Message> {
		match message {
			Message::WindowResized(w, h) => {
				self.window_size = Some((w, h));
				Task::none()
			}
			Message::CursorMoved(x, y) => {
				self.set_focused_by_cursor(x, y);
				Task::none()
			}
			Message::Navigate(p) => {
				self.page = p;
				Task::none()
			}
			Message::UsersFilterChanged(v) => {
				self.users_filter_raw = v;
				Task::none()
			}
			Message::SettingsCategorySelected(cat) => {
				self.settings_category = cat;
				Task::none()
			}
			Message::PlatformSelected(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.default_platform = choice.0;
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::MaxLogItemsChanged(v) => {
				self.max_log_items_raw = v.clone();
				if let Ok(n) = v.trim().parse::<usize>()
					&& n > 0
				{
					let mut gs = self.state.gui_settings().clone();
					gs.max_log_items = n;
					self.state.set_gui_settings(gs);
					for tab in self.state.tabs.values_mut() {
						tab.log.max_items = n;
						while tab.log.items.len() > n {
							tab.log.items.pop_front();
						}
					}
				}
				Task::none()
			}
			Message::SplitLayoutSelected(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.split_layout = choice.0;
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::DragModifierSelected(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.drag_modifier = choice.0;
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::CloseKeyChanged(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.close_key = choice
					.chars()
					.next()
					.map(|c| c.to_ascii_lowercase().to_string())
					.unwrap_or_default();
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::NewKeyChanged(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.new_key = choice
					.chars()
					.next()
					.map(|c| c.to_ascii_lowercase().to_string())
					.unwrap_or_default();
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::ReconnectKeyChanged(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.reconnect_key = choice
					.chars()
					.next()
					.map(|c| c.to_ascii_lowercase().to_string())
					.unwrap_or_default();
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::VimLeftKeyChanged(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.vim_left_key = choice
					.chars()
					.next()
					.map(|c| c.to_ascii_lowercase().to_string())
					.unwrap_or_default();
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::VimDownKeyChanged(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.vim_down_key = choice
					.chars()
					.next()
					.map(|c| c.to_ascii_lowercase().to_string())
					.unwrap_or_default();
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::VimUpKeyChanged(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.vim_up_key = choice
					.chars()
					.next()
					.map(|c| c.to_ascii_lowercase().to_string())
					.unwrap_or_default();
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::VimRightKeyChanged(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.vim_right_key = choice
					.chars()
					.next()
					.map(|c| c.to_ascii_lowercase().to_string())
					.unwrap_or_default();
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::VimNavToggled(val) => {
				let mut gs = self.state.gui_settings().clone();
				gs.keybinds.vim_nav = val;
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::ServerEndpointChanged(v) => {
				self.server_endpoint_quic = v;
				Task::none()
			}
			Message::ServerAuthTokenChanged(v) => {
				self.server_auth_token = v;
				Task::none()
			}
			Message::LocaleSelected(locale) => {
				let mut gs = self.state.gui_settings().clone();
				gs.locale = locale.clone();
				self.state.set_gui_settings(gs);
				rust_i18n::set_locale(&locale);
				Task::none()
			}
			Message::AutoConnectToggled(val) => {
				let mut gs = self.state.gui_settings().clone();
				gs.auto_connect_on_startup = val;
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::ConnectPressed => {
				let mut gs = self.state.gui_settings().clone();
				if !chatty_client_core::ClientConfigV1::server_endpoint_locked() {
					gs.server_endpoint_quic = self.server_endpoint_quic.clone();
				}

				let cfg = match settings::build_client_config(&gs) {
					Ok(c) => c,
					Err(e) => {
						self.toast = Some(e.clone());
						self.state.push_notification(UiNotificationKind::Error, e);
						return Task::none();
					}
				};

				self.state.set_connection_status(ConnectionStatus::Connecting);
				let net = self.net.clone();
				Task::perform(async move { net.connect(cfg).await }, Message::ConnectFinished)
			}
			Message::DisconnectPressed => {
				let net = self.net.clone();
				Task::perform(async move { net.disconnect("user").await }, Message::ConnectFinished)
			}
			Message::ConnectFinished(res) => {
				match res {
					Ok(()) => {
						self.pending_error = None;
						self.toast = None;
					}
					Err(e) => {
						self.pending_error = Some(e.clone());
						self.state.push_notification(UiNotificationKind::Error, e);
					}
				}
				Task::none()
			}
			Message::ThemeSelected(choice) => {
				let mut gs = self.state.gui_settings().clone();
				gs.theme = choice.0;
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::PaneJoinChanged(pane, v) => {
				if let Some(p) = self.panes.get_mut(pane) {
					p.join_raw = v;
				}
				Task::none()
			}
			Message::PaneJoinPressed(pane) => {
				let raw = self.panes.get(pane).map(|p| p.join_raw.clone()).unwrap_or_default();
				let req = JoinRequest { raw };
				let Some(room) = self.state.parse_join_room(&req) else {
					self.toast = Some(t!("invalid_room").to_string());
					return Task::none();
				};
				let tid = self.ensure_tab_for_room(&room);
				if let Some(p) = self.panes.get_mut(pane) {
					p.tab_id = Some(tid);
					p.join_raw = format!("{}:{}", room.platform.as_str(), room.room_id.as_str());
				}
				self.save_ui_layout();

				let net = self.net.clone();
				Task::perform(async move { net.subscribe_room_key(room).await }, move |res| {
					Message::PaneSubscribed(pane, res)
				})
			}
			Message::MessageActionButtonPressed(room, server_msg_id, platform_msg_id, author_id) => {
				if let Some(active) = &self.pending_message_action
					&& active.0 == room
					&& active.1 == server_msg_id
					&& active.2 == platform_msg_id
				{
					self.pending_message_action = None;
					return Task::none();
				}
				self.pending_message_action = Some((room, server_msg_id, platform_msg_id, author_id));
				Task::none()
			}
			Message::ReplyToMessage(_room, server_msg_id, platform_msg_id) => {
				let pane = self.focused_pane;
				if let Some(p) = self.panes.get_mut(pane) {
					p.reply_to_server_message_id = server_msg_id.clone().unwrap_or_default();
					p.reply_to_platform_message_id = platform_msg_id.clone().unwrap_or_default();

					self.insert_mode = true;
					self.insert_target = Some(InsertTarget::Composer);
				}
				self.pending_message_action = None;
				Task::none()
			}
			Message::DeleteMessage(room, server_msg_id, platform_msg_id) => {
				let topic = chatty_domain::RoomTopic::format(&room);
				let cmd = chatty_protocol::pb::Command {
					command: Some(chatty_protocol::pb::command::Command::DeleteMessage(
						chatty_protocol::pb::DeleteMessageCommand {
							topic,
							server_message_id: server_msg_id.clone().unwrap_or_default(),
							platform_message_id: platform_msg_id.clone().unwrap_or_default(),
						},
					)),
				};
				let net = self.net.clone();
				self.pending_message_action = None;
				self.pending_deletion = Some((room.clone(), server_msg_id.clone(), platform_msg_id.clone()));
				self.pending_commands.push(crate::app::model::PendingCommand::Delete {
					room: room.clone(),
					server_message_id: server_msg_id.clone(),
					platform_message_id: platform_msg_id.clone(),
				});
				Task::perform(
					async move {
						let res: Result<(), String> = net.send_command(cmd).await.map_err(|e| e.to_string());
						Message::Sent(res)
					},
					|m| m,
				)
			}
			Message::TimeoutUser(room, user_id) => {
				let topic = chatty_domain::RoomTopic::format(&room);
				let cmd = chatty_protocol::pb::Command {
					command: Some(chatty_protocol::pb::command::Command::TimeoutUser(
						chatty_protocol::pb::TimeoutUserCommand {
							topic,
							user_id: user_id.clone(),
							duration_seconds: 600,
							reason: String::new(),
						},
					)),
				};
				let net = self.net.clone();
				self.pending_message_action = None;
				self.pending_commands.push(crate::app::model::PendingCommand::Timeout {
					room: room.clone(),
					user_id: user_id.clone(),
				});
				Task::perform(
					async move {
						let res: Result<(), String> = net.send_command(cmd).await.map_err(|e| e.to_string());
						Message::Sent(res)
					},
					|m| m,
				)
			}
			Message::BanUser(room, user_id) => {
				let topic = chatty_domain::RoomTopic::format(&room);
				let cmd = chatty_protocol::pb::Command {
					command: Some(chatty_protocol::pb::command::Command::BanUser(
						chatty_protocol::pb::BanUserCommand {
							topic,
							user_id: user_id.clone(),
							reason: String::new(),
						},
					)),
				};
				let net = self.net.clone();
				self.pending_message_action = None;
				self.pending_commands.push(crate::app::model::PendingCommand::Ban {
					room: room.clone(),
					user_id: user_id.clone(),
				});
				Task::perform(
					async move {
						let res: Result<(), String> = net.send_command(cmd).await.map_err(|e| e.to_string());
						Message::Sent(res)
					},
					|m| m,
				)
			}
			Message::PaneSubscribed(_pane, res) => {
				if let Err(e) = res {
					self.toast = Some(e.clone());
					self.state.push_notification(UiNotificationKind::Error, e);
				} else {
					self.save_ui_layout();
				}

				Task::none()
			}
			Message::TabUnsubscribed(room, res) => {
				if let Err(e) = res {
					let msg = format!(
						"{} {}: {}",
						t!("failed_to_unsubscribe"),
						chatty_domain::RoomTopic::format(&room),
						e
					);
					self.toast = Some(msg.clone());
					self.state.push_notification(UiNotificationKind::Error, msg);
				} else {
					// unsubscribed successfully; nothing else to do
				}

				Task::none()
			}
			Message::PaneComposerChanged(pane, v) => {
				if let Some(p) = self.panes.get_mut(pane) {
					p.composer = v;
				}

				self.save_ui_layout();
				Task::none()
			}
			Message::PaneSendPressed(pane) => {
				let Some(room) = self.pane_room(pane) else {
					self.toast = Some(t!("no_active_room").to_string());
					return Task::none();
				};
				if let Some(perms) = self.state.room_permissions.get(&room)
					&& !perms.can_send
				{
					self.toast = Some(t!("cannot_send_room").to_string());
					return Task::none();
				}

				let (text, reply_to_server_message_id, reply_to_platform_message_id) = if let Some(p) = self.panes.get(pane)
				{
					(
						p.composer.trim().to_string(),
						p.reply_to_server_message_id.clone(),
						p.reply_to_platform_message_id.clone(),
					)
				} else {
					return Task::none();
				};

				if text.is_empty() {
					return Task::none();
				}

				if let Some(p) = self.panes.get_mut(pane) {
					p.composer.clear();
				}

				self.save_ui_layout();

				let topic = RoomTopic::format(&room);
				let cmd = pb::Command {
					command: Some(pb::command::Command::SendChat(pb::SendChatCommand {
						topic,
						text: text.clone(),
						reply_to_server_message_id,
						reply_to_platform_message_id,
					})),
				};

				let net = self.net.clone();
				Task::perform(async move { net.send_command(cmd).await.map_err(|e| e.to_string()) }, |res| {
					Message::Sent(res)
				})
			}
			Message::ExportLayoutPressed => {
				let root = self.capture_ui_root();
				let export_path = dirs::home_dir().map(|h| h.join(".chatty").join("ui_layout_export.json"));
				if let Some(p) = export_path.clone() {
					self.pending_export_root = Some(root);
					self.pending_export_path = Some(p);
					self.toast = Some(t!("confirm_export_prompt").to_string());
				} else {
					self.toast = Some(t!("export_failed_no_path").to_string());
				}
				Task::none()
			}
			Message::ChooseExportPathPressed => Task::perform(
				async move { rfd::FileDialog::new().set_file_name("ui_layout_export.json").save_file() },
				Message::LayoutExportPathChosen,
			),
			Message::LayoutExportPathChosen(opt) => {
				self.pending_export_path = opt;
				if self.pending_export_path.is_some() {
					self.toast = Some(t!("export_path_selected").to_string());
				} else {
					self.toast = Some(t!("export_path_selection_canceled").to_string());
				}
				Task::none()
			}
			Message::ImportLayoutPressed => clipboard::read().map(Message::LayoutImportClipboard),
			Message::ImportFromFilePressed => Task::perform(
				async move {
					let pick = rfd::FileDialog::new().pick_file();
					match pick {
						Some(p) => match std::fs::read_to_string(&p) {
							Ok(s) => match serde_json::from_str::<crate::ui::layout::UiRootState>(&s) {
								Ok(root) => Ok(root),
								Err(e) => Err(format!("parse error: {}", e)),
							},
							Err(e) => Err(format!("read error: {}", e)),
						},
						None => Err("canceled".to_string()),
					}
				},
				Message::LayoutImportFileParsed,
			),
			Message::LayoutImportClipboard(opt) => {
				if let Some(txt) = opt {
					match serde_json::from_str::<crate::ui::layout::UiRootState>(&txt) {
						Ok(root) => {
							self.pending_import_root = Some(root);
							self.toast = Some(t!("import_parsed_confirm").to_string());
						}
						Err(e) => {
							self.toast = Some(format!("{}: {}", t!("import_failed"), e));
						}
					}
				} else {
					self.toast = Some(t!("clipboard_empty").to_string());
				}
				Task::none()
			}
			Message::LayoutImportFileParsed(res) => {
				match res {
					Ok(root) => {
						self.pending_import_root = Some(root);
						self.toast = Some(t!("import_parsed_confirm").to_string());
					}
					Err(e) => {
						self.toast = Some(format!("{}: {}", t!("import_failed"), e));
					}
				}
				Task::none()
			}
			Message::ConfirmImport => {
				if let Some(root) = self.pending_import_root.clone() {
					self.apply_ui_root(root.clone());
					crate::ui::layout::save_ui_layout(&root);
					self.pending_import_root = None;
					self.toast = Some(t!("imported_layout").to_string());
				}
				Task::none()
			}
			Message::CancelImport => {
				self.pending_import_root = None;
				self.toast = Some(t!("import_cancelled").to_string());
				Task::none()
			}
			Message::ModalDismissed => {
				if self.pending_import_root.is_some() {
					self.pending_import_root = None;
					self.toast = Some(t!("import_cancelled").to_string());
				} else if self.pending_export_root.is_some() {
					self.pending_export_root = None;
					self.pending_export_path = None;
					self.toast = Some(t!("export_cancelled").to_string());
				}
				Task::none()
			}
			Message::ConfirmExport => {
				if let Some(root) = self.pending_export_root.clone() {
					crate::ui::layout::save_ui_layout(&root);
					if let Some(p) = self.pending_export_path.clone() {
						if let Ok(json_s) = serde_json::to_string_pretty(&root) {
							let _ = std::fs::write(p.clone(), json_s);
							self.pending_export_root = None;
							self.pending_export_path = None;
							self.toast = Some(format!("{} {}", t!("exported_layout"), p.display()));
						} else {
							self.toast = Some(t!("export_failed_serialization").to_string());
						}
					}
					self.pending_export_root = None;
					self.pending_export_path = None;
				}
				Task::none()
			}
			Message::CancelExport => {
				self.pending_export_root = None;
				self.pending_export_path = None;
				self.toast = Some(t!("export_cancelled").to_string());
				Task::none()
			}
			Message::ConfirmReset => {
				crate::ui::layout::delete_ui_layout();
				self.apply_ui_root(crate::ui::layout::UiRootState::default());
				self.pending_reset = false;
				self.toast = Some(t!("layout_reset").to_string());
				Task::none()
			}
			Message::CancelReset => {
				self.pending_reset = false;
				self.toast = Some(t!("reset_cancelled").to_string());
				Task::none()
			}
			Message::CancelError => {
				self.pending_error = None;
				self.toast = None;
				Task::none()
			}
			Message::ResetLayoutPressed => {
				self.pending_reset = true;
				self.toast = Some(t!("confirm_reset_prompt").to_string());
				Task::none()
			}
			Message::Sent(res) => {
				if let Err(e) = res {
					self.toast = Some(e.clone());
					self.state.push_notification(UiNotificationKind::Error, e);
					if let Some(pc) = self.pending_commands.pop()
						&& let crate::app::model::PendingCommand::Delete { .. } = pc
					{
						self.pending_deletion = None;
					}
				}
				Task::none()
			}
			Message::PasteTwitchBlob => clipboard::read().map(|txt| Message::ClipboardRead(ClipboardTarget::Twitch, txt)),
			Message::PasteKickBlob => clipboard::read().map(|txt| Message::ClipboardRead(ClipboardTarget::Kick, txt)),
			Message::ClipboardRead(target, txt) => {
				let Some(txt) = txt.filter(|s| !s.trim().is_empty()) else {
					self.toast = Some(t!("clipboard_empty").to_string());
					return Task::none();
				};
				match target {
					ClipboardTarget::Twitch => self.upsert_identity_from_twitch_blob(txt),
					ClipboardTarget::Kick => self.upsert_identity_from_kick_blob(txt),
				}
				Task::none()
			}
			Message::IdentityUse(id) => {
				let mut gs = self.state.gui_settings().clone();
				gs.active_identity = Some(id);
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::IdentityToggle(id) => {
				let mut gs = self.state.gui_settings().clone();
				if let Some(identity) = gs.identities.iter_mut().find(|i| i.id == id) {
					identity.enabled = !identity.enabled;
					if !identity.enabled && gs.active_identity.as_deref() == Some(identity.id.as_str()) {
						gs.active_identity = None;
					}
					self.state.set_gui_settings(gs);
				}
				Task::none()
			}
			Message::IdentityRemove(id) => {
				let mut gs = self.state.gui_settings().clone();
				gs.identities.retain(|i| i.id != id);
				if gs.active_identity.as_deref() == Some(id.as_str()) {
					gs.active_identity = None;
				}
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::ClearIdentity => {
				let mut gs = self.state.gui_settings().clone();
				gs.active_identity = None;
				self.state.set_gui_settings(gs);
				Task::none()
			}
			Message::NetPolled(ev) => self.update_net_polled(ev),
			Message::PaneClicked(pane) => {
				self.focused_pane = pane;
				self.save_ui_layout();
				Task::none()
			}
			Message::OpenPlatformLogin(platform) => {
				let url = match platform {
					Platform::Twitch => chatty_client_core::TWITCH_LOGIN_URL.to_string(),
					Platform::Kick => chatty_client_core::KICK_LOGIN_URL.to_string(),
					_ => String::new(),
				};

				if url.trim().is_empty() {
					self.toast = Some(t!("settings.no_login_url").to_string());
					return Task::none();
				}

				if let Err(e) = open::that(url) {
					self.toast = Some(format!("{}: {}", t!("settings.open_failed"), e));
				}

				Task::none()
			}
			Message::PaneResized(ev) => {
				self.panes.resize(ev.split, ev.ratio);
				self.save_ui_layout();
				Task::none()
			}
			Message::PaneDragged(ev) => {
				if !self.pane_drag_enabled() {
					return Task::none();
				}
				match ev {
					pane_grid::DragEvent::Picked { pane } => {
						self.focused_pane = pane;
					}
					pane_grid::DragEvent::Dropped { pane, target } => {
						self.panes.drop(pane, target);
						self.focused_pane = pane;
						self.save_ui_layout();
					}
					pane_grid::DragEvent::Canceled { .. } => {}
				}
				Task::none()
			}
			Message::SplitSpiral => {
				self.split_spiral();
				self.save_ui_layout();
				Task::none()
			}
			Message::SplitMasonry => {
				self.split_masonry();
				self.save_ui_layout();
				Task::none()
			}
			Message::SplitPressed => {
				let gs = self.state.gui_settings();
				match gs.split_layout {
					SplitLayoutKind::Spiral => self.split_spiral(),
					SplitLayoutKind::Linear => self.split_linear(),
					SplitLayoutKind::Masonry => self.split_masonry(),
				}

				self.save_ui_layout();
				Task::none()
			}
			Message::ModifiersChanged(modifiers) => {
				self.modifiers = modifiers;
				Task::none()
			}
			Message::CloseFocused => {
				let focused = self.focused_pane;
				let tab_id_opt = self.panes.get(focused).and_then(|p| p.tab_id);
				let room_opt = self.pane_room(focused);
				if let Some((_closed, sibling)) = self.panes.close(focused) {
					self.focused_pane = sibling;

					if let (Some(tid), Some(room)) = (tab_id_opt, room_opt) {
						let still_referenced = self.panes.iter().any(|(_, p)| p.tab_id == Some(tid));
						if !still_referenced {
							self.state.tabs.remove(&tid);
							let room_clone = room.clone();
							let net = self.net.clone();
							return Task::perform(
								async move {
									let res = net.unsubscribe_room_key(room_clone.clone()).await;
									(room_clone, res)
								},
								|(room, res)| Message::TabUnsubscribed(room, res),
							);
						}
					}
				}

				Task::none()
			}
			Message::DismissToast => {
				self.toast = None;
				Task::none()
			}
			Message::CharPressed(ch, modifiers) => {
				let k = settings::get_cloned().keybinds;

				if shortcut_match(modifiers, k.drag_modifier) {
					if first_char_lower(&k.close_key) == ch {
						let focused = self.focused_pane;
						let tab_id_opt = self.panes.get(focused).and_then(|p| p.tab_id);
						let room_opt = self.pane_room(focused);
						if let Some((_closed, sibling)) = self.panes.close(focused) {
							self.focused_pane = sibling;
							if let (Some(tid), Some(room)) = (tab_id_opt, room_opt) {
								let still_referenced = self.panes.iter().any(|(_, p)| p.tab_id == Some(tid));
								if !still_referenced {
									self.state.tabs.remove(&tid);
									let room_clone = room.clone();
									let net = self.net.clone();
									return Task::perform(
										async move {
											let res = net.unsubscribe_room_key(room_clone.clone()).await;
											(room_clone, res)
										},
										|(room, res)| Message::TabUnsubscribed(room, res),
									);
								}
							}
						}
						return Task::none();
					}

					if first_char_lower(&k.new_key) == ch {
						self.split_spiral();
						self.save_ui_layout();
						return Task::none();
					}

					if first_char_lower(&k.reconnect_key) == ch {
						let gs = self.state.gui_settings().clone();

						let cfg = match settings::build_client_config(&gs) {
							Ok(c) => c,
							Err(e) => {
								self.toast = Some(e.clone());
								self.state.push_notification(UiNotificationKind::Error, e);
								return Task::none();
							}
						};

						self.state.set_connection_status(ConnectionStatus::Connecting);
						let net = self.net.clone();
						return Task::perform(async move { net.connect(cfg).await }, |_| Message::ConnectFinished(Ok(())));
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

				if modifiers == iced::keyboard::Modifiers::default() && ch == 'i' {
					let pane = self.focused_pane;
					let mut use_composer = false;
					if let Some(ps) = self.panes.get(pane)
						&& let Some(tid) = ps.tab_id
						&& let Some(tab) = self.state.tabs.get(&tid)
					{
						let room_key = match &tab.target {
							chatty_client_ui::app_state::TabTarget::Room(r) => Some(r.clone()),
							_ => None,
						};
						let can_send = room_key
							.as_ref()
							.and_then(|rk| self.state.room_permissions.get(rk))
							.map(|p| p.can_send)
							.unwrap_or(true);
						let connected = matches!(
							self.state.connection,
							chatty_client_ui::app_state::ConnectionStatus::Connected { .. }
						);
						use_composer = connected && room_key.is_some() && can_send;
					}
					self.insert_mode = true;
					self.insert_target = if use_composer {
						Some(InsertTarget::Composer)
					} else {
						Some(InsertTarget::Join)
					};

					self.toast = Some(t!("insert_mode").to_string());
					let id = if use_composer {
						format!("composer-{:?}", pane)
					} else {
						format!("join-{:?}", pane)
					};

					return iced::widget::operation::focus(id);
				}

				Task::none()
			}
			Message::NamedKeyPressed(named) => {
				use iced::keyboard::key::Named;
				match named {
					Named::Escape => {
						if self.pending_import_root.is_some() {
							self.pending_import_root = None;
							self.toast = Some(t!("import_cancelled").to_string());
						} else if self.pending_export_root.is_some() {
							self.pending_export_root = None;
							self.pending_export_path = None;
							self.toast = Some(t!("export_cancelled").to_string());
						} else if self.insert_mode {
							self.insert_mode = false;
							self.insert_target = None;
							self.toast = Some(t!("insert_mode_exited").to_string());
							return iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus());
						}
						Task::none()
					}
					Named::Enter => {
						if self.insert_mode {
							return Task::none();
						}
						if self.insert_mode {
							let pane = self.focused_pane;
							let room_opt = self.pane_room(pane);
							if let Some(p) = self.panes.get_mut(pane) {
								match self.insert_target {
									Some(InsertTarget::Composer) => {
										let Some(room) = room_opt else {
											self.toast = Some(t!("no_active_room").to_string());
											return Task::none();
										};
										if let Some(perms) = self.state.room_permissions.get(&room)
											&& !perms.can_send
										{
											self.toast = Some(t!("cannot_send_room").to_string());
											return Task::none();
										}

										let (text, reply_to_server_message_id, reply_to_platform_message_id) = {
											let text = p.composer.trim().to_string();
											if text.is_empty() {
												return Task::none();
											}
											let reply_to_server_message_id = p.reply_to_server_message_id.clone();
											let reply_to_platform_message_id = p.reply_to_platform_message_id.clone();
											p.composer.clear();
											(text, reply_to_server_message_id, reply_to_platform_message_id)
										};

										self.save_ui_layout();

										let topic = chatty_domain::RoomTopic::format(&room);
										let cmd = chatty_protocol::pb::Command {
											command: Some(chatty_protocol::pb::command::Command::SendChat(
												chatty_protocol::pb::SendChatCommand {
													topic,
													text: text.clone(),
													reply_to_server_message_id,
													reply_to_platform_message_id,
												},
											)),
										};

										let net = self.net.clone();
										return Task::perform(
											async move { net.send_command(cmd).await.map_err(|e| e.to_string()) },
											Message::Sent,
										);
									}
									Some(InsertTarget::Join) => {
										return Task::none().map(move |_: ()| Message::PaneJoinPressed(pane));
									}
									_ => (),
								}
							}
						}
						Task::none()
					}
					Named::Backspace => {
						if self.insert_mode {
							let pane = self.focused_pane;
							if let Some(p) = self.panes.get_mut(pane) {
								match self.insert_target {
									Some(InsertTarget::Composer) => {
										p.composer.pop();
									}
									Some(InsertTarget::Join) => {
										p.join_raw.pop();
									}
									_ => {}
								}
							}
							self.save_ui_layout();
						}
						Task::none()
					}
					_ => Task::none(),
				}
			}
			Message::NavigatePaneLeft => {
				self.navigate_pane(-1, 0);
				Task::none()
			}
			Message::NavigatePaneDown => {
				self.navigate_pane(0, 1);
				Task::none()
			}
			Message::NavigatePaneUp => {
				self.navigate_pane(0, -1);
				Task::none()
			}
			Message::NavigatePaneRight => {
				self.navigate_pane(1, 0);
				Task::none()
			}
		}
	}

	fn update_net_polled(&mut self, ev: Option<UiEvent>) -> Task<Message> {
		let Some(ev) = ev else {
			self.state
				.push_notification(UiNotificationKind::Warning, t!("network_event_stream_closed").to_string());
			return Task::none();
		};

		if let Some(room) = self.collect_orphaned_tab() {
			let net = self.net.clone();
			return Task::perform(
				async move { (room.clone(), net.unsubscribe_room_key(room).await) },
				|(room, res)| Message::TabUnsubscribed(room, res),
			);
		}

		match ev {
			UiEvent::Connecting => {
				self.state.set_connection_status(ConnectionStatus::Connecting);
			}
			UiEvent::Reconnecting {
				attempt,
				next_retry_in_ms,
			} => {
				self.state.set_connection_status(ConnectionStatus::Reconnecting {
					attempt,
					next_retry_in_ms,
				});
			}
			UiEvent::Connected {
				server_name,
				server_instance_id,
			} => {
				let server = if server_instance_id.trim().is_empty() || server_instance_id == "unknown" {
					server_name
				} else {
					format!("{} ({})", server_name, server_instance_id)
				};
				self.state.set_connection_status(ConnectionStatus::Connected { server });
			}
			UiEvent::Disconnected { reason } => {
				if !reason.trim().is_empty() {
					self.toast = Some(format!("{} {reason}", t!("disconnected_colon")));
				}
				self.state
					.set_connection_status(ConnectionStatus::Disconnected { reason: Some(reason) });
			}
			UiEvent::Error { message } => {
				if !message.trim().is_empty() {
					self.toast = Some(message.clone());
				}
				self.state.push_notification(UiNotificationKind::Error, message);
			}
			UiEvent::ErrorWithServer {
				message,
				server,
				server_instance: _,
			} => {
				let msg = if let Some(s) = server.as_ref() {
					format!("{} (server {})", message, s)
				} else {
					message.clone()
				};
				if !msg.trim().is_empty() {
					self.toast = Some(msg.clone());
				}
				self.state.push_notification(UiNotificationKind::Error, msg);
			}
			UiEvent::ChatMessage {
				topic,
				cursor: _,
				author_login,
				author_display,
				author_id,
				text,
				server_message_id,
				platform_message_id,
				badge_ids,
			} => {
				if let Ok(room) = RoomTopic::parse(&topic) {
					let _tid = self.ensure_tab_for_room(&room);
					let msg = chatty_client_ui::app_state::ChatMessageUi {
						time: SystemTime::now(),
						platform: room.platform,
						room: room.clone(),
						server_message_id,
						author_id,
						user_login: author_login,
						user_display: author_display,
						text,
						badge_ids,
						platform_message_id,
					};
					let _ = self.state.push_message(msg);
				} else {
					self.state
						.push_notification(UiNotificationKind::Warning, format!("{}: {topic}", t!("unparseable_topic")));
				}
			}
			UiEvent::TopicLagged {
				topic,
				cursor: _,
				dropped,
				detail,
			} => {
				if let Ok(room) = RoomTopic::parse(&topic) {
					let _ = self.state.push_lagged(&room, dropped, Some(detail));
				}
			}
			UiEvent::RoomPermissions {
				topic,
				can_send,
				can_reply,
				can_delete,
				can_timeout,
				can_ban,
				is_moderator,
				is_broadcaster,
			} => {
				if let Ok(room) = RoomTopic::parse(&topic) {
					self.state.room_permissions.insert(
						room,
						chatty_client_ui::app_state::RoomPermissions {
							can_send,
							can_reply,
							can_delete,
							can_timeout,
							can_ban,
							is_moderator,
							is_broadcaster,
						},
					);
				}
			}
			UiEvent::AssetBundle {
				topic,
				cache_key,
				etag,
				provider,
				scope,
				emotes,
				badges,
			} => {
				let ck = cache_key.clone();
				self.state.asset_bundles.insert(
					ck.clone(),
					chatty_client_ui::app_state::AssetBundleUi {
						cache_key: ck.clone(),
						etag,
						provider,
						scope,
						emotes,
						badges,
					},
				);

				if let Ok(room) = RoomTopic::parse(&topic) {
					let keys = self.state.room_asset_cache_keys.entry(room).or_default();
					if !keys.contains(&ck) {
						keys.push(ck.clone());
					}
				}

				{
					let img_cache = self.image_cache.clone();
					let sender = self.image_fetch_sender.clone();
					if let Some(bundle) = self.state.asset_bundles.get(&ck) {
						for em in &bundle.emotes {
							let url = em.image_url.clone();
							let img_cache_cl = img_cache.clone();
							if img_cache_cl.lock().unwrap().contains(&url) {
								continue;
							}
							let _ = sender.try_send(url);
						}
						for bd in &bundle.badges {
							let url = bd.image_url.clone();
							let img_cache_cl = img_cache.clone();
							if img_cache_cl.lock().unwrap().contains(&url) {
								continue;
							}
							let _ = sender.try_send(url);
						}
					}
				}
			}
			UiEvent::CommandResult { status, detail } => {
				self.state
					.push_notification(UiNotificationKind::Info, format!("command status={status}: {detail}"));
				if let Some((room, server_id, platform_id)) = self.pending_deletion.take() {
					if status == 1 {
						self.state.remove_message(&room, server_id.as_deref(), platform_id.as_deref());
						self.state
							.push_notification(UiNotificationKind::Info, "deleted message".to_string());
					} else {
						self.pending_commands.retain(|pc| match pc {
							crate::app::model::PendingCommand::Delete {
								room: r,
								server_message_id: s,
								platform_message_id: p,
							} => !(r == &room && s.as_ref() == server_id.as_ref() && p.as_ref() == platform_id.as_ref()),
							_ => true,
						});
					}
				}
				if status == pb::command_result::Status::Ok as i32 {
					self.pending_commands.retain(|pc| {
						!matches!(
							pc,
							crate::app::model::PendingCommand::Timeout { .. }
								| crate::app::model::PendingCommand::Ban { .. }
						)
					});
				} else {
					// on failure, keep commands but notify user (detail already shown)
				}
			}
		}

		Task::perform(recv_next(self.net_rx.clone()), Message::NetPolled)
	}
}
