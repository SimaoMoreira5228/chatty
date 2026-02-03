#![forbid(unsafe_code)]

use std::time::Duration;

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UiNotificationKind {
	Info,
	Success,
	Warning,
	Error,
}

#[derive(Debug, Clone)]
pub struct UiNotification {
	pub kind: UiNotificationKind,
	pub message: String,
}

#[derive(Debug, Clone)]
pub enum ToasterMessage {
	Show(String),
	Dismiss,
}

#[derive(Debug, Clone, Default)]
pub struct Toaster {
	pub current: Option<String>,
}

impl Toaster {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn update(&mut self, _app: &mut Chatty, message: ToasterMessage) -> Task<Message> {
		match message {
			ToasterMessage::Show(msg) => {
				self.current = Some(msg);
				Task::perform(async { tokio::time::sleep(Duration::from_secs(3)).await }, |_| {
					Message::ToasterMessage(ToasterMessage::Dismiss)
				})
			}
			ToasterMessage::Dismiss => {
				self.current = None;
				Task::none()
			}
		}
	}
}
