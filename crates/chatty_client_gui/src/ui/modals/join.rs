use chatty_domain::Platform;
use iced::widget::{button, column, container, pick_list, row, rule, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Task};
use rust_i18n::t;

use crate::app::{Chatty, Message, PlatformChoice};
use crate::theme;

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
			JoinModalMessage::Submit => app.update_join_modal_submit(),
			JoinModalMessage::Cancel => app.update_modal_dismissed(),
		}
	}

	pub fn view<'a>(&'a self, _app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		let placeholder = t!("main.join_room_placeholder").to_string();
		let inner = container(
			column![
				text(t!("main.join_room_title").to_string()).color(palette.text).size(18),
				rule::horizontal(1),
				row![
					text(t!("settings.default_platform").to_string()).color(palette.text_dim),
					pick_list(&PlatformChoice::ALL[..], Some(PlatformChoice(self.platform)), |p| {
						Message::OverlayMessage(crate::ui::modals::OverlayMessage::Join(JoinModalMessage::PlatformSelected(
							p.0,
						)))
					})
				]
				.spacing(12)
				.align_y(Alignment::Center),
				text_input(&placeholder, &self.input)
					.on_input(|v| Message::OverlayMessage(crate::ui::modals::OverlayMessage::Join(
						JoinModalMessage::InputChanged(v)
					)))
					.on_submit(Message::OverlayMessage(crate::ui::modals::OverlayMessage::Join(
						JoinModalMessage::Submit
					)))
					.padding(10),
				row![
					button(text(t!("main.join_button"))).on_press(Message::OverlayMessage(
						crate::ui::modals::OverlayMessage::Join(JoinModalMessage::Submit)
					)),
					button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(
						crate::ui::modals::OverlayMessage::Join(JoinModalMessage::Cancel)
					)),
				]
				.spacing(12)
				.align_y(Alignment::Center),
			]
			.spacing(16)
			.padding(20),
		)
		.width(320)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.tooltip_bg)),
			border: Border {
				color: palette.border,
				width: 1.0,
				radius: 8.0.into(),
			},
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
