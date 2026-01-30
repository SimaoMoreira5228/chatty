use std::time::Duration;

use iced::widget::{container, text};
use iced::{Background, Border, Element, Shadow, Task};

use crate::app::{Chatty, Message};
use crate::theme;

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

	pub fn view<'a>(&'a self, palette: theme::Palette) -> Element<'a, Message> {
		if let Some(msg) = &self.current {
			container(text(msg).color(palette.text))
				.padding(10)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.accent_blue)),
					border: Border {
						color: palette.border,
						width: 1.0,
						radius: 8.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				})
				.into()
		} else {
			iced::widget::space().into()
		}
	}
}
