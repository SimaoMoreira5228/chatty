use iced::widget::{button, column, container, row, rule, text};
use iced::{Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use crate::app::features::settings::{SettingsMessage, SettingsView};
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::app::types::SettingsCategory;
use crate::theme;

pub mod accounts;
pub mod diagnostics;
pub mod general;
pub mod keybinds;
pub mod server;

impl SettingsView {
	pub fn view<'a>(&'a self, app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		let categories = {
			let mut c = column![text(t!("settings.title")).color(palette.text), rule::horizontal(1)].spacing(10);
			for cat in SettingsCategory::ALL {
				let active = cat == self.category;
				let color = if active { palette.text } else { palette.text_dim };
				c = c.push(
					button(text(t!(cat.label_key())).color(color))
						.on_press(Message::Settings(SettingsMessage::CategorySelected(cat))),
				);
			}
			c
		};

		let right: Element<'a, Message> = match self.category {
			SettingsCategory::General => general::view(app, palette),
			SettingsCategory::Keybinds => keybinds::view(app, palette),
			SettingsCategory::Server => server::view(app, palette),
			SettingsCategory::Accounts => accounts::view(app, palette),
			SettingsCategory::Diagnostics => diagnostics::view(app, palette),
		};

		row![
			container(categories)
				.width(180)
				.height(Length::Fill)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.panel_bg_2)),
					border: Border::default(),
					shadow: Shadow::default(),
					snap: false,
				}),
			container(right)
				.width(Length::Fill)
				.height(Length::Fill)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.panel_bg)),
					border: Border::default(),
					shadow: Shadow::default(),
					snap: false,
				}),
		]
		.spacing(10)
		.padding(12)
		.into()
	}
}
