#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{div, px};

use gpui_component::StyledExt;
use gpui_component::button::Button;

use crate::ui::theme;

use super::SettingsPage;

pub(super) fn render(this: &mut SettingsPage, t: &theme::Theme, cx: &mut Context<SettingsPage>) -> gpui::Div {
	let identity_rows = {
		let identities = this.settings.identities.clone();
		identities.into_iter().map(|identity| {
			let identity_id = identity.id.clone();
			let identity_id_use = identity_id.clone();
			let identity_id_toggle = identity_id.clone();
			let identity_id_remove = identity_id.clone();
			let identity_key = SettingsPage::identity_key(&identity_id);
			let enabled = identity.enabled;
			let is_active = this.settings.active_identity.as_deref() == Some(identity_id.as_str());
			let status = if is_active { "Active" } else { "Inactive" };
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().w(px(180.0)).child(identity.display_name.clone()))
				.child(div().text_xs().text_color(t.text_dim).child(format!("{}", identity.platform)))
				.child(
					div()
						.text_xs()
						.text_color(if enabled { t.text_dim } else { t.text_muted })
						.child(status),
				)
				.child(
					Button::new(("identity-use", identity_key))
						.px_2()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_xs()
						.label("Use")
						.on_click(cx.listener(move |this, _ev, _window, cx| {
							this.settings.active_identity = Some(identity_id_use.clone());
							this.update_settings(cx);
							this.push_system_notice("Identity set as active", cx);
						})),
				)
				.child(
					Button::new(("identity-toggle", identity_key))
						.px_2()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_xs()
						.label(if enabled { "Disable" } else { "Enable" })
						.on_click(cx.listener(move |this, _ev, _window, cx| {
							if let Some(identity) = this.settings.identities.iter_mut().find(|i| i.id == identity_id_toggle)
							{
								identity.enabled = !identity.enabled;
								if !identity.enabled
									&& this.settings.active_identity.as_deref() == Some(identity.id.as_str())
								{
									this.settings.active_identity = None;
								}
								this.update_settings(cx);
								this.push_system_notice("Identity toggled", cx);
							}
						})),
				)
				.child(
					Button::new(("identity-remove", identity_key))
						.px_2()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_xs()
						.label("Remove")
						.on_click(cx.listener(move |this, _ev, _window, cx| {
							this.settings.identities.retain(|i| i.id != identity_id_remove);
							if this.settings.active_identity.as_deref() == Some(identity_id_remove.as_str()) {
								this.settings.active_identity = None;
							}
							this.update_settings(cx);
							this.push_system_notice("Identity removed", cx);
						})),
				)
		})
	};

	div()
		.flex()
		.flex_col()
		.gap_3()
		.child(div().text_sm().font_semibold().child("Accounts"))
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Active Identity:"))
				.child(div().text_sm().text_color(t.text_dim).child(this.active_identity_label())),
		)
		.child(
			div().flex().items_center().gap_2().child(
				Button::new("identity-clear")
					.px_3()
					.py_1()
					.rounded_sm()
					.bg(t.button_bg)
					.text_color(t.button_text)
					.text_sm()
					.label("Clear Identity")
					.on_click(cx.listener(|this, _ev, _window, cx| {
						this.settings.active_identity = None;
						this.update_settings(cx);
						this.push_system_notice("Active identity cleared", cx);
					})),
			),
		)
		.child(
			div()
				.text_xs()
				.text_color(t.text_dim)
				.child("Paste a Twitch or Kick login blob to populate identity."),
		)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Twitch Login:"))
				.child(
					Button::new("twitch-login-open")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Open Twitch Login")
						.on_click(cx.listener(|_this, _ev, _window, _cx| {
							SettingsPage::open_url(&SettingsPage::twitch_login_url());
						})),
				)
				.child(
					Button::new("twitch-login-paste")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Paste Login Blob")
						.on_click(cx.listener(|this, _ev, window, cx| {
							this.paste_twitch_blob(window, cx);
						})),
				),
		)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Kick Login:"))
				.child(
					Button::new("kick-login-open")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Open Kick Login")
						.on_click(cx.listener(|_this, _ev, _window, _cx| {
							SettingsPage::open_url(&SettingsPage::kick_login_url());
						})),
				)
				.child(
					Button::new("kick-login-paste")
						.px_3()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.text_color(t.button_text)
						.text_sm()
						.label("Paste Kick Blob")
						.on_click(cx.listener(|this, _ev, window, cx| {
							this.paste_kick_blob(window, cx);
						})),
				),
		)
		.child(div().flex().flex_col().gap_2().children(identity_rows))
}
