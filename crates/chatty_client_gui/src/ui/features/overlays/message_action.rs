use iced::widget::{button, column, container, text};
use iced::{Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use super::overlay::wrap_overlay_at;
use crate::app::features::overlays::{MessageActionMenu, MessageActionMenuMessage, OverlayMessage};
use crate::app::message::Message;
use crate::theme;

pub struct MessageActionViewModel {
	pub can_reply: bool,
	pub can_delete: bool,
	pub can_timeout: bool,
	pub can_ban: bool,
	pub cursor_pos: Option<(f32, f32)>,
}

impl MessageActionMenu {
	pub fn view_model(&self, _app: &crate::app::model::Chatty) -> MessageActionViewModel {
		MessageActionViewModel {
			can_reply: true,
			can_delete: true,
			can_timeout: true,
			can_ban: true,
			cursor_pos: self.cursor_pos,
		}
	}

	pub fn view(&self, vm: MessageActionViewModel, palette: theme::Palette) -> Element<'_, Message> {
		let mut items = column![].spacing(6);

		if vm.can_reply {
			items = items.push(button(text(t!("actions.reply"))).on_press(Message::OverlayMessage(
				OverlayMessage::MessageAction(MessageActionMenuMessage::Reply),
			)));
		}
		if vm.can_delete {
			items = items.push(button(text(t!("actions.delete"))).on_press(Message::OverlayMessage(
				OverlayMessage::MessageAction(MessageActionMenuMessage::Delete),
			)));
		}
		if vm.can_timeout {
			items = items.push(button(text(t!("actions.timeout"))).on_press(Message::OverlayMessage(
				OverlayMessage::MessageAction(MessageActionMenuMessage::Timeout),
			)));
		}
		if vm.can_ban {
			items = items.push(button(text(t!("actions.ban"))).on_press(Message::OverlayMessage(
				OverlayMessage::MessageAction(MessageActionMenuMessage::Ban),
			)));
		}

		let container_el = container(items.padding(8)).style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.panel_bg)),
			border: Border {
				color: palette.border,
				width: 1.0,
				radius: 8.0.into(),
			},
			shadow: Shadow::default(),
			snap: false,
		});

		let content = container_el.width(Length::Shrink).height(Length::Shrink).into();

		wrap_overlay_at(content, palette, vm.cursor_pos)
	}
}
