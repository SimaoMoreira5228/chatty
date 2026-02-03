#![forbid(unsafe_code)]

use iced::widget::{container, text};
use iced::{Background, Border, Element, Shadow};

use crate::app::features::toaster::Toaster;
use crate::app::message::Message;
use crate::theme;

impl Toaster {
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
