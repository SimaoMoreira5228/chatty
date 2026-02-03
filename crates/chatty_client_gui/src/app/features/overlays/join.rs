#![forbid(unsafe_code)]

use chatty_domain::Platform;
use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;

#[derive(Debug, Clone)]
pub enum JoinModalMessage {
	InputChanged(String),
	PlatformSelected(Platform),
	Submit,
	Cancel,
}

#[derive(Debug, Clone)]
pub struct JoinModal {
	pub input: String,
	pub platform: Platform,
}

impl JoinModal {
	pub fn new(platform: Platform) -> Self {
		Self {
			input: String::new(),
			platform,
		}
	}

	pub fn update(&mut self, app: &mut Chatty, message: JoinModalMessage) -> Task<Message> {
		match message {
			JoinModalMessage::InputChanged(v) => {
				self.input = v;
				Task::none()
			}
			JoinModalMessage::PlatformSelected(p) => {
				self.platform = p;
				Task::none()
			}
			JoinModalMessage::Submit => app.update_join_modal_submit(self.clone()),
			JoinModalMessage::Cancel => app.update_modal_dismissed(),
		}
	}
}
