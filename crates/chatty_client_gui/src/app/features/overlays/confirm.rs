#![forbid(unsafe_code)]

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;

#[derive(Debug, Clone)]
pub enum ConfirmModalMessage {
	Confirm,
	Cancel,
}

#[derive(Debug, Clone)]
pub enum ConfirmModalKind {
	DeleteMessage,
	TimeoutUser(String),
	BanUser(String),
}

#[derive(Debug, Clone)]
pub struct ConfirmModal {
	pub room: chatty_domain::RoomKey,
	pub kind: ConfirmModalKind,
	pub server_message_id: Option<String>,
	pub platform_message_id: Option<String>,
}

impl ConfirmModal {
	pub fn new_delete(
		room: chatty_domain::RoomKey,
		server_message_id: Option<String>,
		platform_message_id: Option<String>,
	) -> Self {
		Self {
			room,
			kind: ConfirmModalKind::DeleteMessage,
			server_message_id,
			platform_message_id,
		}
	}

	pub fn new_timeout(room: chatty_domain::RoomKey, user_id: String) -> Self {
		Self {
			room,
			kind: ConfirmModalKind::TimeoutUser(user_id),
			server_message_id: None,
			platform_message_id: None,
		}
	}

	pub fn new_ban(room: chatty_domain::RoomKey, user_id: String) -> Self {
		Self {
			room,
			kind: ConfirmModalKind::BanUser(user_id),
			server_message_id: None,
			platform_message_id: None,
		}
	}

	pub fn update(&mut self, app: &mut Chatty, message: ConfirmModalMessage) -> Task<Message> {
		match message {
			ConfirmModalMessage::Confirm => {
				app.state.ui.overlay_dismissed = true;
				app.update_confirm_modal_confirmed(self.clone())
			}
			ConfirmModalMessage::Cancel => {
				app.state.ui.overlay_dismissed = true;
				app.state.ui.active_overlay = None;
				Task::none()
			}
		}
	}
}
