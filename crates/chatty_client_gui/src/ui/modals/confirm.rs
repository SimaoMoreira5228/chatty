use iced::widget::{button, column, container, row, rule, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Task};
use rust_i18n::t;

use crate::app::{Chatty, Message};
use crate::theme;

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
			ConfirmModalMessage::Confirm => app.update_confirm_modal_confirmed(self.clone()),
			ConfirmModalMessage::Cancel => {
				app.state.ui.active_overlay = None;
				Task::none()
			}
		}
	}

	pub fn view<'a>(&'a self, _app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		let (title, description) = match &self.kind {
			ConfirmModalKind::DeleteMessage => {
				(t!("confirm.delete_title").to_string(), t!("confirm.delete_desc").to_string())
			}
			ConfirmModalKind::TimeoutUser(uid) => (
				t!("confirm.timeout_title").to_string(),
				format!("{} {}", t!("confirm.timeout_desc"), uid),
			),
			ConfirmModalKind::BanUser(uid) => (
				t!("confirm.ban_title").to_string(),
				format!("{} {}", t!("confirm.ban_desc"), uid),
			),
		};

		let inner = container(column![
			text(title).color(palette.text).size(16),
			rule::horizontal(1),
			text(description).color(palette.text_dim),
			row![
				button(text(t!("confirm_label"))).on_press(Message::OverlayMessage(
					crate::ui::modals::OverlayMessage::Confirm(ConfirmModalMessage::Confirm)
				)),
				button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(
					crate::ui::modals::OverlayMessage::Confirm(ConfirmModalMessage::Cancel)
				)),
			]
			.spacing(8)
			.align_y(Alignment::Center),
		])
		.padding(12)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.tooltip_bg)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		});

		container(container(inner).center_x(Length::Fill).center_y(Length::Fill))
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			})
			.into()
	}
}
