use iced::widget::{button, column, container, pick_list, row, rule, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use super::overlay::wrap_overlay;
use crate::app::features::overlays::{JoinModal, JoinModalMessage, OverlayMessage};
use crate::app::message::Message;
use crate::theme;
use chatty_domain::Platform;

const PLATFORM_OPTIONS: [Platform; 2] = [Platform::Twitch, Platform::Kick];

impl JoinModal {
	pub fn view<'a>(&'a self, palette: theme::Palette) -> Element<'a, Message> {
		let input = text_input(&t!("main.join_room_placeholder"), &self.input)
			.on_input(|v| Message::OverlayMessage(OverlayMessage::Join(JoinModalMessage::InputChanged(v))));

		let platform_picker = pick_list(&PLATFORM_OPTIONS[..], Some(self.platform), |p| {
			Message::OverlayMessage(OverlayMessage::Join(JoinModalMessage::PlatformSelected(p)))
		});

		let submit_btn = button(text(t!("main.join_button")))
			.on_press(Message::OverlayMessage(OverlayMessage::Join(JoinModalMessage::Submit)));
		let cancel_btn = button(text(t!("cancel_label")))
			.on_press(Message::OverlayMessage(OverlayMessage::Join(JoinModalMessage::Cancel)));

		let body_col = column![
			text(t!("main.join_room_title")).color(palette.text),
			rule::horizontal(1),
			row![text(t!("settings.default_platform")).color(palette.text_dim), platform_picker]
				.spacing(12)
				.align_y(Alignment::Center),
			input,
			row![cancel_btn, submit_btn].spacing(8).align_y(Alignment::Center),
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
