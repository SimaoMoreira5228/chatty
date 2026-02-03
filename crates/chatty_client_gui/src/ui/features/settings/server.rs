use iced::widget::{button, column, row, rule, scrollable, svg, text, text_input};
use iced::{Alignment, Element, Length};
use rust_i18n::t;

use crate::app::features::settings::SettingsMessage;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::assets::svg_handle;
use crate::theme;

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let endpoint_locked = chatty_client_core::ClientConfigV1::server_endpoint_locked();
	let default_endpoint = chatty_client_core::ClientConfigV1::default_server_endpoint_quic();

	let hmac_enabled = chatty_client_core::HMAC_ENABLED;
	let hmac_key = chatty_client_core::HMAC_KEY;
	let show_hmac_input = hmac_enabled && hmac_key.is_empty();

	let endpoint_row = if endpoint_locked {
		row![
			text(format!("{} ", t!("settings.endpoint"))).color(palette.text_dim),
			text(default_endpoint).color(palette.text).width(Length::Fill)
		]
	} else {
		row![
			text(format!("{} ", t!("settings.endpoint"))).color(palette.text_dim),
			text_input("quic://host:port", &app.state.ui.server_endpoint_quic)
				.on_input(|v| Message::Settings(SettingsMessage::ServerEndpointChanged(v)))
				.width(Length::Fill),
		]
	};

	let hmac_row: Option<iced::Element<'_, Message>> = if show_hmac_input {
		Some(
			row![
				text(t!("settings.hmac_token")).color(palette.text_dim),
				text_input(t!("settings.optional").to_string().as_str(), &app.state.ui.server_auth_token)
					.on_input(|v| Message::Settings(SettingsMessage::ServerAuthTokenChanged(v)))
					.width(Length::Fill),
			]
			.spacing(12)
			.align_y(Alignment::Center)
			.into(),
		)
	} else {
		None
	};
	scrollable(
		column![
			text(t!("settings.server")).color(palette.text),
			rule::horizontal(1),
			endpoint_row,
			if let Some(h) = hmac_row { h } else { text("").into() },
			row![
				button(
					row![
						svg(svg_handle("connect.svg")).width(14).height(14),
						text(t!("settings.connect"))
					]
					.spacing(4)
				)
				.on_press(Message::Net(crate::app::message::NetMessage::ConnectPressed)),
				button(
					row![
						svg(svg_handle("disconnect.svg")).width(14).height(14),
						text(t!("settings.disconnect"))
					]
					.spacing(4)
				)
				.on_press(Message::Net(crate::app::message::NetMessage::DisconnectPressed)),
			]
			.spacing(10),
		]
		.spacing(12)
		.padding(12),
	)
	.into()
}
