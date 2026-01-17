#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{Window, div, px};

use gpui_component::StyledExt;
use gpui_component::button::Button;
use gpui_component::input::Input;

use crate::ui::theme;

use super::SettingsPage;

pub(super) fn render(
	this: &mut SettingsPage,
	t: &theme::Theme,
	_window: &mut Window,
	cx: &mut Context<SettingsPage>,
) -> gpui::Div {
	let connection_status = this.app_state.as_ref().map(|app| app.read(cx).connection.clone());
	let (status_label, status_detail) = match connection_status {
		Some(crate::ui::app_state::ConnectionStatus::Connected { server }) => {
			("Connected".to_string(), Some(format!("server: {server}")))
		}
		Some(crate::ui::app_state::ConnectionStatus::Connecting) => ("Connecting".to_string(), None),
		Some(crate::ui::app_state::ConnectionStatus::Reconnecting {
			attempt,
			next_retry_in_ms,
		}) => (
			"Reconnecting".to_string(),
			Some(format!("attempt {attempt}, retry in {}s", next_retry_in_ms / 1000)),
		),
		Some(crate::ui::app_state::ConnectionStatus::Disconnected { reason }) => {
			("Disconnected".to_string(), reason.map(|r| format!("reason: {r}")))
		}
		None => ("Unknown".to_string(), None),
	};

	let status_row = div()
		.flex()
		.items_center()
		.gap_2()
		.child(div().w(px(160.0)).child("Connection:"))
		.child(div().text_sm().text_color(t.text_dim).child(status_label));

	let status_detail_row = status_detail
		.map(|detail| div().text_xs().text_color(t.text_muted).child(detail))
		.unwrap_or_else(div);

	div()
		.flex()
		.flex_col()
		.gap_3()
		.child(div().text_sm().font_semibold().child("Diagnostics"))
		.child(status_row)
		.child(status_detail_row)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(
					Button::new("diag-connect")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Connect")
						.on_click(cx.listener(|this, _ev, _window, cx| {
							this.connect_now(cx);
						})),
				)
				.child(
					Button::new("diag-disconnect")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Disconnect")
						.on_click(cx.listener(|this, _ev, _window, cx| {
							this.disconnect_now(cx);
						})),
				)
				.child(
					Button::new("diag-lag")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Inject Lag Marker")
						.on_click(cx.listener(|this, _ev, _window, cx| {
							this.inject_lag_marker(cx);
						})),
				),
		)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Test Room:"))
				.child(
					Input::new(&this.diagnostic_room_input)
						.bg(t.panel_bg)
						.text_color(t.text)
						.border_1()
						.border_color(t.border)
						.rounded_sm(),
				),
		)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(
					Button::new("diag-subscribe")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Subscribe")
						.on_click(cx.listener(|this, _ev, _window, cx| {
							let raw = this.diagnostic_room_input.read(cx).value().to_string();
							let rooms = this.parse_rooms_list(&raw);
							if rooms.is_empty() {
								this.push_system_notice("Diagnostics: subscribe requested (no rooms)", cx);
							} else {
								this.push_system_notice("Diagnostics: subscribe requested", cx);
							}
							let Some(net) = this.net_controller.clone() else {
								return;
							};
							cx.spawn(async move |_, _cx| {
								for room in rooms {
									let _ = net.subscribe_room_key(room).await;
								}
							})
							.detach();
						})),
				)
				.child(
					Button::new("diag-unsubscribe")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Unsubscribe")
						.on_click(cx.listener(|this, _ev, _window, cx| {
							let raw = this.diagnostic_room_input.read(cx).value().to_string();
							let rooms = this.parse_rooms_list(&raw);
							if rooms.is_empty() {
								this.push_system_notice("Diagnostics: unsubscribe requested (no rooms)", cx);
							} else {
								this.push_system_notice("Diagnostics: unsubscribe requested", cx);
							}
							let Some(net) = this.net_controller.clone() else {
								return;
							};
							cx.spawn(async move |_, _cx| {
								for room in rooms {
									let _ = net.unsubscribe_room_key(room).await;
								}
							})
							.detach();
						})),
				),
		)
}
