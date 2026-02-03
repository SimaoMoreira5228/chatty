use iced::widget::{button, column, container, row, rule, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use super::overlay::wrap_overlay;
use crate::app::features::overlays::{ConfirmModal, ConfirmModalKind, ConfirmModalMessage, OverlayMessage};
use crate::app::message::Message;
use crate::theme;

impl ConfirmModal {
	pub fn view<'a>(&'a self, palette: theme::Palette) -> Element<'a, Message> {
		let (title, body, confirm_label) = match &self.kind {
			ConfirmModalKind::DeleteMessage => (
				t!("confirm.delete_title").to_string(),
				t!("confirm.delete_desc").to_string(),
				t!("actions.delete").to_string(),
			),
			ConfirmModalKind::TimeoutUser(_) => (
				t!("confirm.timeout_title").to_string(),
				t!("confirm.timeout_desc").to_string(),
				t!("actions.timeout").to_string(),
			),
			ConfirmModalKind::BanUser(_) => (
				t!("confirm.ban_title").to_string(),
				t!("confirm.ban_desc").to_string(),
				t!("actions.ban").to_string(),
			),
		};

		let confirm_btn = button(text(confirm_label))
			.on_press(Message::OverlayMessage(OverlayMessage::Confirm(ConfirmModalMessage::Confirm)));
		let cancel_btn = button(text(t!("cancel_label")))
			.on_press(Message::OverlayMessage(OverlayMessage::Confirm(ConfirmModalMessage::Cancel)));

		let body_col = column![
			text(title).color(palette.text),
			rule::horizontal(1),
			text(body).color(palette.text_dim),
			row![cancel_btn, confirm_btn].spacing(8).align_y(Alignment::Center),
		]
		.spacing(12)
		.padding(12);

		let content = container(body_col)
			.width(Length::Shrink)
			.height(Length::Shrink)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.panel_bg)),
				border: Border {
					color: palette.border,
					width: 1.0,
					radius: 10.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			});

		wrap_overlay(content.into(), palette)
	}
}
