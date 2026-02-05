use iced::widget::{Space, container, mouse_area, stack};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow};

use crate::app::message::Message;
use crate::theme;

pub fn wrap_overlay<'a>(content: Element<'a, Message>, palette: theme::Palette) -> Element<'a, Message> {
	wrap_overlay_at(content, palette, None)
}

pub fn wrap_overlay_at<'a>(
	content: Element<'a, Message>,
	palette: theme::Palette,
	position: Option<(f32, f32)>,
) -> Element<'a, Message> {
	let backdrop = container(Space::new().width(Length::Fill).height(Length::Fill))
		.width(Length::Fill)
		.height(Length::Fill)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(Color {
				a: 0.72,
				..palette.app_bg
			})),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		});
	let backdrop = mouse_area(backdrop).on_press(Message::ModalDismissed);

	let mut positioned = container(content).width(Length::Fill).height(Length::Fill);

	if let Some((x, y)) = position {
		let offset_x = x.max(0.0) + 8.0;
		let offset_y = y.max(0.0) + 8.0;
		positioned = positioned
			.align_x(Alignment::Start)
			.align_y(Alignment::Start)
			.padding(Padding {
				top: offset_y,
				left: offset_x,
				right: 0.0,
				bottom: 0.0,
			});
	} else {
		positioned = positioned.center_x(Length::Fill).center_y(Length::Fill);
	}

	stack(vec![backdrop.into(), positioned.into()])
		.width(Length::Fill)
		.height(Length::Fill)
		.into()
}
