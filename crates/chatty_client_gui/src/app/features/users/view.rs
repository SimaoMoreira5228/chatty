#![forbid(unsafe_code)]

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;

#[derive(Debug, Clone)]
pub enum UsersViewMessage {
	FilterChanged(String),
}

#[derive(Debug, Clone)]
pub struct UsersView {
	pub filter: String,
}

impl UsersView {
	pub fn new() -> Self {
		Self { filter: String::new() }
	}

	pub fn update(&mut self, _app: &mut Chatty, message: UsersViewMessage) -> Task<Message> {
		match message {
			UsersViewMessage::FilterChanged(v) => {
				self.filter = v;
				Task::none()
			}
		}
	}
}
