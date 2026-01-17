#![forbid(unsafe_code)]

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Rgba, SharedString, Window, div, px};
use gpui_component::Icon;

use crate::ui::theme;

#[derive(Clone)]
pub struct StatusChip {
	pub label: SharedString,
	pub color: Rgba,
}

pub struct TopbarButton<T> {
	pub id: &'static str,
	pub label: SharedString,
	#[allow(clippy::type_complexity)]
	pub on_click: Arc<dyn Fn(&mut T, &mut Window, &mut Context<T>) + Send + Sync + 'static>,
}

pub fn render_topbar<T, F, G>(
	title: impl Into<SharedString>,
	status: Option<StatusChip>,
	_window: &mut Window,
	cx: &mut Context<T>,
	on_users: F,
	on_settings: G,
	extra_button: Option<TopbarButton<T>>,
) -> impl IntoElement
where
	F: Fn(&mut T, &mut Window, &mut Context<T>) + Send + Sync + 'static,
	G: Fn(&mut T, &mut Window, &mut Context<T>) + Send + Sync + 'static,
	T: 'static,
{
	let t = theme::theme();
	let title = title.into();
	let users_cb = Arc::new(on_users);
	let settings_cb = Arc::new(on_settings);
	let status_chip = status.map(|chip| {
		div()
			.id("conn-status")
			.px_2()
			.py_1()
			.rounded_full()
			.bg(t.app_bg)
			.border_1()
			.border_color(t.border)
			.text_color(chip.color)
			.text_sm()
			.child(chip.label)
	});

	let extra_button = extra_button.map(|button| {
		let on_click = button.on_click.clone();
		div()
			.id(button.id)
			.px_1()
			.py_1()
			.rounded_sm()
			.bg(t.button_bg)
			.hover({
				let t = t.clone();
				move |d| d.bg(t.button_hover_bg)
			})
			.cursor_pointer()
			.text_color(t.button_text)
			.text_xs()
			.child(button.label)
			.on_click({
				let on_click = on_click.clone();
				cx.listener(move |this, _ev, window, cx| {
					(on_click)(this, window, cx);
				})
			})
	});

	div()
		.id("top-bar")
		.flex()
		.flex_row()
		.items_center()
		.justify_between()
		.flex_none()
		.w_full()
		.h(px(32.0))
		.px_3()
		.bg(t.panel_bg_2)
		.text_color(t.text)
		.border_b_1()
		.border_color(t.border)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().text_sm().text_color(t.text_dim).child(title))
				.children(status_chip),
		)
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(
					div()
						.id("btn-users")
						.px_2()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.hover({
							let t = t.clone();
							move |d| d.bg(t.button_hover_bg)
						})
						.cursor_pointer()
						.text_color(t.button_text)
						.child(Icon::empty().path("users.svg"))
						.on_click({
							let users_cb = users_cb.clone();
							cx.listener(move |this, _ev, window, cx| {
								(users_cb)(this, window, cx);
							})
						}),
				)
				.child(
					div()
						.id("btn-settings")
						.px_2()
						.py_1()
						.rounded_sm()
						.bg(t.button_bg)
						.hover({
							let t = t.clone();
							move |d| d.bg(t.button_hover_bg)
						})
						.cursor_pointer()
						.text_color(t.button_text)
						.child(Icon::empty().path("settings.svg"))
						.on_click({
							let settings_cb = settings_cb.clone();
							cx.listener(move |this, _ev, window, cx| {
								(settings_cb)(this, window, cx);
							})
						}),
				)
				.children(extra_button),
		)
}
