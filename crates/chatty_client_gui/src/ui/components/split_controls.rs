#![forbid(unsafe_code)]

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Window, div, px};

use crate::ui::theme;
use gpui_component::Icon;

pub fn render_split_controls<T, F, G, H>(
	_window: &mut Window,
	cx: &mut Context<T>,
	on_add: F,
	on_popout: G,
	on_close: H,
) -> impl IntoElement
where
	F: Fn(&mut T, &mut Window, &mut Context<T>) + Send + Sync + 'static,
	G: Fn(&mut T, &mut Window, &mut Context<T>) + Send + Sync + 'static,
	H: Fn(&mut T, &mut Window, &mut Context<T>) + Send + Sync + 'static,
	T: 'static,
{
	let t = theme::theme();
	let add_cb = Arc::new(on_add);
	let popout_cb = Arc::new(on_popout);
	let close_cb = Arc::new(on_close);

	div()
		.id("split-controls")
		.h(px(36.0))
		.px_3()
		.flex()
		.items_center()
		.justify_end()
		.gap_2()
		.flex_none()
		.w_full()
		.bg(t.panel_bg_2)
		.border_t_1()
		.border_color(t.border)
		.child(
			div()
				.id("split-add")
				.px_2()
				.py_1()
				.rounded_sm()
				.bg(t.button_bg)
				.hover({
					let t = t.clone();
					move |d| d.bg(t.button_hover_bg)
				})
				.text_color(t.button_text)
				.cursor_pointer()
				.child(Icon::empty().path("split.svg"))
				.on_click({
					let add_cb = add_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(add_cb)(this, window, cx);
					})
				}),
		)
		.child(
			div()
				.id("split-popout")
				.px_2()
				.py_1()
				.rounded_sm()
				.bg(t.button_bg)
				.hover({
					let t = t.clone();
					move |d| d.bg(t.button_hover_bg)
				})
				.text_color(t.button_text)
				.cursor_pointer()
				.child(Icon::empty().path("open-in-new.svg"))
				.on_click({
					let popout_cb = popout_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(popout_cb)(this, window, cx);
					})
				}),
		)
		.child(
			div()
				.id("split-close")
				.px_2()
				.py_1()
				.rounded_sm()
				.bg(t.button_bg)
				.hover({
					let t = t.clone();
					move |d| d.bg(t.button_hover_bg)
				})
				.text_color(t.button_text)
				.cursor_pointer()
				.child(Icon::empty().path("close.svg"))
				.on_click({
					let close_cb = close_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(close_cb)(this, window, cx);
					})
				}),
		)
}
