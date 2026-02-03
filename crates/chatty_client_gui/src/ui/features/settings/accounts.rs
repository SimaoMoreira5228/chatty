use chatty_domain::Platform;
use iced::widget::{button, column, row, rule, scrollable, svg, text};
use iced::{Alignment, Element, Length};
use rust_i18n::t;

use crate::app::features::settings::SettingsMessage;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::assets::svg_handle;
use crate::theme;

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let gs = app.state.gui_settings().clone();
	let mut identities_col = column![].spacing(8);
	for identity in gs.identities.iter() {
		identities_col = identities_col.push(
			row![
				text(identity.display_name.clone()).color(palette.text),
				text(format!("{:?}", identity.platform)).color(palette.text_dim),
				button(row![svg(svg_handle("close.svg")).width(14).height(14), text(t!("action.remove"))].spacing(4))
					.on_press(Message::Settings(SettingsMessage::IdentityRemove(identity.id.clone()))),
			]
			.spacing(10)
			.align_y(Alignment::Center),
		);
	}

	let twitch_login_cfg = chatty_client_core::TWITCH_LOGIN_URL;
	let kick_login_cfg = chatty_client_core::KICK_LOGIN_URL;

	let mut body_col = column![
		text(t!("settings.accounts")).color(palette.text),
		rule::horizontal(1),
		text(t!("settings.paste_login_blob_help")).color(palette.text_muted),
	]
	.spacing(12);

	if !twitch_login_cfg.is_empty() {
		body_col = body_col.push(
			row![
				text(t!("settings.twitch_login")).color(palette.text_dim).width(Length::Fill),
				button(
					row![
						svg(svg_handle("open-in-new.svg")).width(14).height(14),
						text(t!("settings.open_login_page"))
					]
					.spacing(4)
				)
				.on_press(Message::Settings(SettingsMessage::OpenPlatformLogin(Platform::Twitch))),
				button(text(t!("settings.paste_login_blob"))).on_press(Message::Settings(SettingsMessage::PasteTwitchBlob)),
			]
			.spacing(10)
			.align_y(Alignment::Center),
		);
	}

	if !kick_login_cfg.is_empty() {
		body_col = body_col.push(
			row![
				text(t!("settings.kick_login")).color(palette.text_dim).width(Length::Fill),
				button(
					row![
						svg(svg_handle("open-in-new.svg")).width(14).height(14),
						text(t!("settings.open_login_page"))
					]
					.spacing(4)
				)
				.on_press(Message::Settings(SettingsMessage::OpenPlatformLogin(Platform::Kick))),
				button(text(t!("settings.paste_login_blob"))).on_press(Message::Settings(SettingsMessage::PasteKickBlob)),
			]
			.spacing(10)
			.align_y(Alignment::Center),
		);
	}

	body_col = body_col
		.push(rule::horizontal(1))
		.push(text(t!("settings.identities")).color(palette.text_dim))
		.push(identities_col);

	scrollable(body_col.padding(12)).into()
}
