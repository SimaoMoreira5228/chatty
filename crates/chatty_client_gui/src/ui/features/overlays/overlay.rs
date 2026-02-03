use iced::widget::{container, mouse_area};
use iced::{Background, Border, Color, Element, Length, Shadow};

use crate::app::message::Message;
use crate::theme;

pub fn wrap_overlay<'a>(content: Element<'a, Message>, palette: theme::Palette) -> Element<'a, Message> {
	mouse_area(
		container(content)
			.width(Length::Fill)
			.height(Length::Fill)
			.center_x(Length::Fill)
			.center_y(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(Color {
					a: 0.72,
					..palette.app_bg
				})),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			}),
	)
	.on_press(Message::ModalDismissed)
	.into()
}
