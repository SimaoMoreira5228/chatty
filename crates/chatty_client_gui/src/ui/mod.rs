#![forbid(unsafe_code)]

pub mod layout;
mod main_view;
mod settings_view;
mod topbar;
mod users_view;

use iced::widget::{column, container, stack};
use iced::{Background, Border, Element, Length, Shadow};

use crate::app::{Chatty, Message, Page};
use crate::theme;

pub fn view(app: &Chatty) -> Element<'_, Message> {
	let palette = theme::palette(app.state.gui_settings().theme);

	let topbar = topbar::view(app, palette);
	let toast_bar = topbar::toast_bar(app, palette);

	let content: Element<'_, Message> = match app.page {
		Page::Main => main_view::view(app, palette),
		Page::Settings => settings_view::view(app, palette),
		Page::Users => users_view::view(app, palette),
	};

	let root = column![
		topbar,
		toast_bar,
		container(content)
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.app_bg)),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			}),
	]
	.spacing(0)
	.width(Length::Fill)
	.height(Length::Fill);

	let mut layers: Vec<Element<'_, Message>> = vec![container(root).width(Length::Fill).height(Length::Fill).into()];

	if app.page == Page::Main
		&& let Some(menu) = main_view::message_action_menu(app, palette)
	{
		layers.push(menu);
	}

	if let Some(modal) = settings_view::modal(app, palette) {
		layers.push(modal);
	}

	let layered = stack(layers).width(Length::Fill).height(Length::Fill);

	container(layered)
		.width(Length::Fill)
		.height(Length::Fill)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.app_bg)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		})
		.into()
}
