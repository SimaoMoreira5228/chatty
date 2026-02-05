#![forbid(unsafe_code)]

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;

#[derive(Debug, Clone)]
pub enum MessageActionMenuMessage {
	Reply,
	Delete,
	Timeout,
	Ban,
}

#[derive(Debug, Clone)]
pub struct MessageActionMenu {
	pub room: chatty_domain::RoomKey,
	pub server_message_id: Option<String>,
	pub platform_message_id: Option<String>,
	pub author_id: Option<String>,
	pub cursor_pos: Option<(f32, f32)>,
}

impl MessageActionMenu {
	pub fn new(
		room: chatty_domain::RoomKey,
		server_message_id: Option<String>,
		platform_message_id: Option<String>,
		author_id: Option<String>,
		cursor_pos: Option<(f32, f32)>,
	) -> Self {
		Self {
			room,
			server_message_id,
			platform_message_id,
			author_id,
			cursor_pos,
		}
	}

	pub fn update(&mut self, app: &mut Chatty, message: MessageActionMenuMessage) -> Task<Message> {
		app.state.ui.overlay_dismissed = true;
		match message {
			MessageActionMenuMessage::Reply => app.update_reply_to_message(
				self.room.clone(),
				self.server_message_id.clone(),
				self.platform_message_id.clone(),
			),
			MessageActionMenuMessage::Delete => app.update_delete_message(
				self.room.clone(),
				self.server_message_id.clone(),
				self.platform_message_id.clone(),
			),
			MessageActionMenuMessage::Timeout => {
				if let Some(uid) = &self.author_id {
					app.update_timeout_user(self.room.clone(), uid.clone())
				} else {
					Task::none()
				}
			}
			MessageActionMenuMessage::Ban => {
				if let Some(uid) = &self.author_id {
					app.update_ban_user(self.room.clone(), uid.clone())
				} else {
					Task::none()
				}
			}
		}
	}
}
