#![forbid(unsafe_code)]

pub mod components;
pub mod layout;
mod main_view;
pub mod modals;

pub mod popped_view;
pub mod settings;
pub mod tab_view;
mod topbar;
pub mod users_view;
pub mod vim;

use iced::widget::{column, container, stack};
use iced::{Background, Border, Element, Length, Shadow};

use crate::app::{Chatty, Message, Page};
use crate::theme;

pub fn view(app: &Chatty, window: iced::window::Id) -> Element<'_, Message> {
	if let Some(win_model) = app.state.popped_windows.get(&window)
		&& let Some(tab_id) = win_model.active_tab
	{
		return popped_view::view(app, tab_id);
	}

	let palette = theme::palette(&app.state.gui_settings().theme, &app.state.custom_themes);

	let topbar = topbar::view(app, palette);

	let content: Element<'_, Message> = match app.state.ui.page {
		Page::Main => main_view::view(app, palette),
		Page::Settings => app.state.ui.settings_view.view(app, palette),
		Page::Users => app.state.ui.users_view.view(app, palette),
	};

	let root = column![
		topbar,
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

	if let Some(overlay) = &app.state.ui.active_overlay {
		layers.push(overlay.view(app, palette));
	}

	layers.push(app.state.ui.toaster.view(palette));

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
