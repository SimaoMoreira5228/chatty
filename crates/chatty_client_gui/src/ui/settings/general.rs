use iced::widget::{column, pick_list, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element};
use rust_i18n::t;

use crate::app::{Chatty, Message, PlatformChoice, SplitLayoutChoice, ThemeChoice};
use crate::theme;
use crate::ui::settings::SettingsMessage;

include!(concat!(env!("OUT_DIR"), "/locales_generated.rs"));

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let mut all_themes = ThemeChoice::all();
	for name in app.state.custom_themes.keys() {
		all_themes.push(ThemeChoice(theme::ThemeKind::Custom(name.clone())));
	}

	let current_theme = ThemeChoice(app.state.gui_settings().theme.clone());
	let theme_picker = pick_list(all_themes, Some(current_theme), |c| {
		Message::SettingsMessage(SettingsMessage::ThemeSelected(c))
	});

	let current_split = SplitLayoutChoice(app.state.gui_settings().split_layout);
	let split_picker = pick_list(&SplitLayoutChoice::ALL[..], Some(current_split), |c| {
		Message::SettingsMessage(SettingsMessage::SplitLayoutSelected(c))
	});

	let platform_picker = pick_list(
		&PlatformChoice::ALL[..],
		Some(PlatformChoice(app.state.gui_settings().default_platform)),
		|c| Message::SettingsMessage(SettingsMessage::PlatformSelected(c)),
	);

	let gs = app.state.gui_settings().clone();

	scrollable(
		column![
			text(t!("settings.general")).color(palette.text),
			rule::horizontal(1),
			row![text(t!("settings.theme")).color(palette.text_dim), theme_picker]
				.spacing(12)
				.align_y(Alignment::Center),
			row![text(t!("settings.default_split")).color(palette.text_dim), split_picker]
				.spacing(12)
				.align_y(Alignment::Center),
			row![text(t!("settings.default_platform")).color(palette.text_dim), platform_picker]
				.spacing(12)
				.align_y(Alignment::Center),
			row![
				text(t!("settings.max_log_items")).color(palette.text_dim),
				text_input("2000", &app.state.ui.max_log_items_raw)
					.on_input(|v| Message::SettingsMessage(SettingsMessage::MaxLogItemsChanged(v))),
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.auto_connect")).color(palette.text_dim),
				iced::widget::checkbox(gs.auto_connect_on_startup)
					.on_toggle(|v| Message::SettingsMessage(SettingsMessage::AutoConnectToggled(v))),
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![text(t!("settings.locale")).color(palette.text_dim), {
				let locales: &'static [&'static str] = AVAILABLE_LOCALES;
				let selected: Option<&'static str> = locales
					.iter()
					.copied()
					.find(|s| *s == gs.locale)
					.or_else(|| locales.first().copied());
				pick_list(locales, selected, |s: &'static str| {
					Message::SettingsMessage(SettingsMessage::LocaleSelected(s.to_string()))
				})
			}]
			.spacing(12)
			.align_y(Alignment::Center),
		]
		.spacing(12)
		.padding(12),
	)
	.into()
}
