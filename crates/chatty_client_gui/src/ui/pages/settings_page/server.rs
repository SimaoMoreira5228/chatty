#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{div, px};

use gpui_component::StyledExt;
use gpui_component::button::Button;
use gpui_component::input::Input;

use crate::ui::theme;
use chatty_util::endpoint::validate_quic_endpoint;

use super::SettingsPage;

pub(super) fn render(
	this: &mut SettingsPage,
	t: &theme::Theme,
	endpoint_locked: bool,
	cx: &mut Context<SettingsPage>,
) -> gpui::Div {
	let default_endpoint = chatty_client_core::ClientConfigV1::default_server_endpoint_quic();
	let endpoint_raw = this.server_endpoint_input.read(cx).value().to_string();
	let endpoint_value = endpoint_raw.trim();
	let endpoint_error = if endpoint_locked || endpoint_value.is_empty() {
		None
	} else {
		validate_quic_endpoint(endpoint_value).err()
	};

	let reset_button = if endpoint_locked {
		div()
	} else {
		div().child(
			Button::new("server-reset")
				.px_3()
				.py_1()
				.rounded_sm()
				.bg(t.button_bg)
				.text_color(t.button_text)
				.text_sm()
				.label("Reset to Default")
				.on_click(cx.listener(|this, _ev, window, cx| {
					let default_endpoint = chatty_client_core::ClientConfigV1::default_server_endpoint_quic();
					this.settings.server_endpoint_quic = default_endpoint.to_string();
					this.server_endpoint_input
						.update(cx, |state, cx| state.set_value(default_endpoint.to_string(), window, cx));
					this.update_settings(cx);
				})),
		)
	};

	let status_line = if endpoint_locked {
		div()
			.text_xs()
			.text_color(t.text_dim)
			.child("Server endpoint is locked in this build.")
	} else if let Some(err) = endpoint_error.as_ref() {
		div()
			.text_xs()
			.text_color(t.text_muted)
			.child(format!("Invalid endpoint: {err}"))
	} else {
		div()
	};

	div()
		.flex()
		.flex_col()
		.gap_3()
		.child(div().text_sm().font_semibold().child("Server"))
		.child(
			div()
				.text_xs()
				.text_color(t.text_dim)
				.child(format!("Default server: {}", default_endpoint)),
		)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Server Endpoint:"))
				.child(
					Input::new(&this.server_endpoint_input)
						.bg(t.panel_bg)
						.text_color(t.text)
						.border_1()
						.border_color(t.border)
						.rounded_sm()
						.disabled(endpoint_locked),
				)
				.child(reset_button),
		)
		.child(status_line)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("HMAC Token:"))
				.child(
					Input::new(&this.server_auth_token_input)
						.bg(t.panel_bg)
						.text_color(t.text)
						.border_1()
						.border_color(t.border)
						.rounded_sm(),
				),
		)
		.child(
			div()
				.text_xs()
				.text_color(t.text_dim)
				.child("Optional HMAC token for private servers."),
		)
}
