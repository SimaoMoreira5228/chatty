use iced::widget::{button, column, container, pick_list, row, rule, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use crate::app::features::settings::SettingsMessage;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::app::types::ShortcutKeyChoice;
use crate::theme;

fn single_char(s: &str) -> String {
	s.chars()
		.next()
		.map(|c| c.to_ascii_lowercase().to_string())
		.unwrap_or_default()
}

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let keybinds = &app.state.gui_settings().keybinds;
	let drag_modifier_picker = pick_list(
		&ShortcutKeyChoice::ALL[..],
		Some(ShortcutKeyChoice(keybinds.drag_modifier)),
		|c| Message::Settings(SettingsMessage::DragModifierSelected(c)),
	);

	let close_key_input = text_input("q", &keybinds.close_key)
		.on_input(|s: String| Message::Settings(SettingsMessage::CloseKeyChanged(single_char(&s))))
		.width(Length::FillPortion(1));
	let new_key_input = text_input("n", &keybinds.new_key)
		.on_input(|s: String| Message::Settings(SettingsMessage::NewKeyChanged(single_char(&s))))
		.width(Length::FillPortion(1));
	let reconnect_key_input = text_input("r", &keybinds.reconnect_key)
		.on_input(|s: String| Message::Settings(SettingsMessage::ReconnectKeyChanged(single_char(&s))))
		.width(Length::FillPortion(1));

	let close_invalid = keybinds.close_key.trim().is_empty();
	let new_invalid = keybinds.new_key.trim().is_empty();
	let reconnect_invalid = keybinds.reconnect_key.trim().is_empty();
	let vim_left_invalid = keybinds.vim_left_key.trim().is_empty();
	let vim_down_invalid = keybinds.vim_down_key.trim().is_empty();
	let vim_up_invalid = keybinds.vim_up_key.trim().is_empty();
	let vim_right_invalid = keybinds.vim_right_key.trim().is_empty();

	let red = iced::Color::from_rgb(0.9, 0.2, 0.2);

	scrollable(
		column![
			text(t!("settings.keybinds")).color(palette.text),
			rule::horizontal(1),
			text(t!("settings.pane_drag_drop")).color(palette.text_dim),
			row![
				text(t!("settings.hold_to_drag")).color(palette.text_dim),
				drag_modifier_picker
			]
			.spacing(12)
			.align_y(Alignment::Center),
			text(t!("settings.split_actions")).color(palette.text_dim),
			row![
				text(t!("settings.close_split")).color(palette.text_dim),
				container(close_key_input).padding(2).style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if close_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if close_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.new_split")).color(palette.text_dim),
				container(new_key_input).padding(2).style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if new_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if new_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.reconnect")).color(palette.text_dim),
				container(reconnect_key_input)
					.padding(2)
					.style(move |_theme| container::Style {
						text_color: Some(palette.text),
						background: Some(Background::Color(palette.surface_bg)),
						border: Border {
							color: if reconnect_invalid { red } else { palette.border },
							width: 1.0,
							radius: 6.0.into(),
						},
						shadow: Shadow::default(),
						snap: false,
					}),
				if reconnect_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			text(t!("settings.shortcuts_help")).color(palette.text_muted),
			row![
				text(t!("settings.vim_navigation")).color(palette.text_dim),
				iced::widget::checkbox(keybinds.vim_nav).on_toggle(|v| Message::Settings(SettingsMessage::VimNavToggled(v)))
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_left")).color(palette.text_dim),
				container(
					text_input("h", &keybinds.vim_left_key)
						.on_input(|s: String| Message::Settings(SettingsMessage::VimLeftKeyChanged(single_char(&s))))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_left_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_left_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_down")).color(palette.text_dim),
				container(
					text_input("j", &keybinds.vim_down_key)
						.on_input(|s: String| Message::Settings(SettingsMessage::VimDownKeyChanged(single_char(&s))))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_down_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_down_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_up")).color(palette.text_dim),
				container(
					text_input("k", &keybinds.vim_up_key)
						.on_input(|s: String| Message::Settings(SettingsMessage::VimUpKeyChanged(single_char(&s))))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_up_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_up_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_right")).color(palette.text_dim),
				container(
					text_input("l", &keybinds.vim_right_key)
						.on_input(|s: String| Message::Settings(SettingsMessage::VimRightKeyChanged(single_char(&s))))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_right_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_right_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			text(t!("settings.vim_nav_help")).color(palette.text_muted),
			row![
				button(text(t!("export_layout_title"))).on_press(Message::Settings(SettingsMessage::ExportLayoutPressed)),
				button(text(t!("settings.import_layout_clipboard")))
					.on_press(Message::Settings(SettingsMessage::ImportLayoutPressed)),
				button(text(t!("settings.import_from_file")))
					.on_press(Message::Settings(SettingsMessage::ImportFromFilePressed)),
				button(text(t!("settings.reset_layout"))).on_press(Message::Settings(SettingsMessage::ResetLayoutPressed)),
			]
			.spacing(8)
			.align_y(Alignment::Center),
			text(t!("settings.export_description")).color(palette.text_muted),
		]
		.spacing(12)
		.padding(12),
	)
	.into()
}
