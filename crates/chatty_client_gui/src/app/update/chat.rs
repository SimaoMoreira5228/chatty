use iced::Task;

use crate::app::message::ChatMessage;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::app::types::{InsertTarget, PendingCommand};

impl Chatty {
	pub fn update_chat_message(&mut self, message: ChatMessage) -> Task<Message> {
		match message {
			ChatMessage::MessageActionButtonPressed(room, s_id, p_id, a_id) => {
				self.update_message_action_button_pressed(room, s_id, p_id, a_id)
			}
			ChatMessage::ReplyToMessage(room, s_id, p_id) => self.update_reply_to_message(room, s_id, p_id),
			ChatMessage::DeleteMessage(room, s_id, p_id) => self.update_delete_message(room, s_id, p_id),
			ChatMessage::TimeoutUser(room, user_id) => self.update_timeout_user(room, user_id),
			ChatMessage::BanUser(room, user_id) => self.update_ban_user(room, user_id),
			ChatMessage::Sent(res) => self.update_sent(res),
			ChatMessage::MessageTextEdit(key, action) => self.update_message_text_edit(key, action),
		}
	}

	pub fn update_message_action_button_pressed(
		&mut self,
		room: chatty_domain::RoomKey,
		server_msg_id: Option<String>,
		platform_msg_id: Option<String>,
		author_id: Option<String>,
	) -> Task<Message> {
		self.state.ui.active_overlay = Some(crate::app::features::overlays::ActiveOverlay::MessageAction(
			crate::app::features::overlays::MessageActionMenu::new(
				room,
				server_msg_id,
				platform_msg_id,
				author_id,
				self.state.ui.last_cursor_pos,
			),
		));

		Task::none()
	}

	pub fn update_reply_to_message(
		&mut self,
		room: chatty_domain::RoomKey,
		server_msg_id: Option<String>,
		platform_msg_id: Option<String>,
	) -> Task<Message> {
		let Some(tab) = self.selected_tab_mut() else {
			return Task::none();
		};
		let Some(pane) = tab.focused_pane else {
			return Task::none();
		};

		if let Some(p) = tab.panes.get_mut(pane) {
			p.reply_to_server_message_id = server_msg_id.clone().unwrap_or_default();
			p.reply_to_platform_message_id = platform_msg_id.clone().unwrap_or_default();
			p.reply_to_room = Some(room);
			if self.state.gui_settings().keybinds.vim_nav {
				self.state.ui.vim.enter_insert_mode(InsertTarget::Composer);
			} else {
				self.state.ui.vim.exit_insert_mode();
			}
		}

		self.state.ui.active_overlay = None;
		let focus_cmd = iced::widget::operation::focus(format!("composer-{:?}", pane));
		Task::batch(vec![focus_cmd])
	}

	pub fn update_delete_message(
		&mut self,
		room: chatty_domain::RoomKey,
		server_msg_id: Option<String>,
		platform_msg_id: Option<String>,
	) -> Task<Message> {
		self.state.ui.active_overlay = Some(crate::app::features::overlays::ActiveOverlay::Confirm(
			crate::app::features::overlays::ConfirmModal::new_delete(room, server_msg_id, platform_msg_id),
		));
		Task::none()
	}

	pub fn execute_delete_message(
		&mut self,
		room: chatty_domain::RoomKey,
		server_msg_id: Option<String>,
		platform_msg_id: Option<String>,
	) -> Task<Message> {
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

		let net = self.net_effects.clone();
		self.state.ui.active_overlay = None;
		self.pending_commands.push(PendingCommand::Delete {
			room: room.clone(),
			server_message_id: server_msg_id.clone(),
			platform_message_id: platform_msg_id.clone(),
		});

		self.rebuild_pending_delete_keys();
		Task::perform(
			async move {
				let res: Result<(), String> = net.send_command(cmd).await.map_err(|e| e.to_string());
				Message::Chat(crate::app::message::ChatMessage::Sent(res))
			},
			|m| m,
		)
	}

	pub fn update_timeout_user(&mut self, room: chatty_domain::RoomKey, user_id: String) -> Task<Message> {
		self.state.ui.active_overlay = Some(crate::app::features::overlays::ActiveOverlay::Confirm(
			crate::app::features::overlays::ConfirmModal::new_timeout(room, user_id),
		));
		Task::none()
	}

	pub fn execute_timeout_user(&mut self, room: chatty_domain::RoomKey, user_id: String) -> Task<Message> {
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
		let net = self.net_effects.clone();
		self.state.ui.active_overlay = None;
		self.pending_commands.push(PendingCommand::Timeout {
			room: room.clone(),
			user_id: user_id.clone(),
		});
		Task::perform(
			async move {
				let res: Result<(), String> = net.send_command(cmd).await.map_err(|e| e.to_string());
				Message::Chat(crate::app::message::ChatMessage::Sent(res))
			},
			|m| m,
		)
	}

	pub fn update_ban_user(&mut self, room: chatty_domain::RoomKey, user_id: String) -> Task<Message> {
		self.state.ui.active_overlay = Some(crate::app::features::overlays::ActiveOverlay::Confirm(
			crate::app::features::overlays::ConfirmModal::new_ban(room, user_id),
		));
		Task::none()
	}

	pub fn execute_ban_user(&mut self, room: chatty_domain::RoomKey, user_id: String) -> Task<Message> {
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
		let net = self.net_effects.clone();
		self.state.ui.active_overlay = None;
		self.pending_commands.push(PendingCommand::Ban {
			room: room.clone(),
			user_id: user_id.clone(),
		});
		Task::perform(
			async move {
				let res: Result<(), String> = net.send_command(cmd).await.map_err(|e| e.to_string());
				Message::Chat(crate::app::message::ChatMessage::Sent(res))
			},
			|m| m,
		)
	}

	pub fn update_confirm_modal_confirmed(&mut self, modal: crate::app::features::overlays::ConfirmModal) -> Task<Message> {
		let room = modal.room;
		let kind = modal.kind;
		let server_msg_id = modal.server_message_id;
		let platform_msg_id = modal.platform_message_id;

		match kind {
			crate::app::features::overlays::ConfirmModalKind::DeleteMessage => {
				self.execute_delete_message(room, server_msg_id, platform_msg_id)
			}
			crate::app::features::overlays::ConfirmModalKind::TimeoutUser(user_id) => {
				self.execute_timeout_user(room, user_id)
			}
			crate::app::features::overlays::ConfirmModalKind::BanUser(user_id) => self.execute_ban_user(room, user_id),
		}
	}

	pub fn update_sent(&mut self, res: Result<(), String>) -> Task<Message> {
		if let Err(e) = res {
			let t = self.report_error(e);
			let _ = self.pending_commands.pop();
			self.rebuild_pending_delete_keys();
			return t;
		}
		Task::none()
	}
}
