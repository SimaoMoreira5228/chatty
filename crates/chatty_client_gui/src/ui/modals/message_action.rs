use iced::widget::{button, column, container, mouse_area, opaque, row, space, stack, text};
use iced::{Background, Border, Element, Length, Shadow, Task};
use rust_i18n::t;

use crate::app::{Chatty, Message};
use crate::theme;

#[derive(Debug, Clone)]
pub enum MessageActionMenuMessage {
	Reply,
	Delete,
	Timeout,
	Ban,
	Dismiss,
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
			MessageActionMenuMessage::Dismiss => app.update_dismiss_message_action(),
		}
	}

	pub fn view<'a>(&'a self, app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		let (win_w, win_h) = app.state.ui.window_size.unwrap_or((800.0, 600.0));
		let (raw_x, raw_y) = self
			.cursor_pos
			.or(app.state.ui.last_cursor_pos)
			.unwrap_or((win_w * 0.5, win_h * 0.5));

		let menu_w = 220.0;
		let menu_h = 160.0;
		let x = raw_x.clamp(8.0, (win_w - menu_w - 8.0).max(8.0));
		let y = raw_y.clamp(8.0, (win_h - menu_h - 8.0).max(8.0));

		let mut actions = column![].spacing(6);
		actions = actions.push(button(text(t!("actions.reply"))).on_press(Message::OverlayMessage(
			crate::ui::modals::OverlayMessage::MessageAction(MessageActionMenuMessage::Reply),
		)));

		if let Some(perms) = app.state.room_permissions.get(&self.room) {
			if perms.can_delete {
				actions = actions.push(button(text(t!("actions.delete"))).on_press(Message::OverlayMessage(
					crate::ui::modals::OverlayMessage::MessageAction(MessageActionMenuMessage::Delete),
				)));
			}
			if perms.can_timeout && self.author_id.is_some() {
				actions = actions.push(button(text(t!("actions.timeout"))).on_press(Message::OverlayMessage(
					crate::ui::modals::OverlayMessage::MessageAction(MessageActionMenuMessage::Timeout),
				)));
			}
			if perms.can_ban && self.author_id.is_some() {
				actions = actions.push(button(text(t!("actions.ban"))).on_press(Message::OverlayMessage(
					crate::ui::modals::OverlayMessage::MessageAction(MessageActionMenuMessage::Ban),
				)));
			}
		}

		let menu = container(actions.padding(6)).style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.panel_bg_2)),
			border: Border {
				color: palette.border,
				width: 1.0,
				radius: 6.0.into(),
			},
			shadow: Shadow::default(),
			snap: false,
		});

		let top = space().height(Length::Fixed(y));
		let left = space().width(Length::Fixed(x));
		let right = space().width(Length::Fill);
		let bottom = space().height(Length::Fill);

		let row = row![left, opaque(menu), right].height(Length::Shrink);
		let overlay = column![top, row, bottom].width(Length::Fill).height(Length::Fill);

		let backdrop = container(mouse_area(space().width(Length::Fill).height(Length::Fill)).on_press(
			Message::OverlayMessage(crate::ui::modals::OverlayMessage::MessageAction(
				MessageActionMenuMessage::Dismiss,
			)),
		))
		.width(Length::Fill)
		.height(Length::Fill);

		stack([backdrop.into(), overlay.into()])
			.width(Length::Fill)
			.height(Length::Fill)
			.into()
	}
}
