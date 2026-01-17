#![forbid(unsafe_code)]

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{MouseButton, ScrollHandle, Window, div, point, px};
use gpui_component::Icon;

use crate::ui::theme;

#[derive(Debug, Clone)]
pub struct TabItem<ID: Clone> {
	pub id: ID,
	pub title: String,
	pub active: bool,
	pub pinned: bool,
}

pub fn render_tab_strip<ID, T, F, G, H, I, J, K, L>(
	tabs: Vec<TabItem<ID>>,
	scroll_handle: ScrollHandle,
	_window: &mut Window,
	cx: &mut Context<T>,
	on_select: F,
	on_close: G,
	on_add: H,
	on_drag_start: I,
	on_drop: J,
	on_toggle_pin: K,
	on_rename: L,
) -> impl IntoElement
where
	ID: Clone + PartialEq + 'static,
	F: Fn(&mut T, ID, &mut Window, &mut Context<T>) + 'static,
	G: Fn(&mut T, ID, &mut Window, &mut Context<T>) + 'static,
	H: Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
	I: Fn(&mut T, ID, &mut Window, &mut Context<T>) + 'static,
	J: Fn(&mut T, ID, &mut Window, &mut Context<T>) + 'static,
	K: Fn(&mut T, ID, &mut Window, &mut Context<T>) + 'static,
	L: Fn(&mut T, ID, &mut Window, &mut Context<T>) + 'static,
	T: 'static,
{
	let t = theme::theme();
	let select_cb = Rc::new(on_select);
	let close_cb = Rc::new(on_close);
	let add_cb = Rc::new(on_add);
	let drag_start_cb = Rc::new(on_drag_start);
	let drop_cb = Rc::new(on_drop);
	let pin_cb = Rc::new(on_toggle_pin);
	let rename_cb = Rc::new(on_rename);

	let scroll_left = {
		let scroll_handle = scroll_handle.clone();
		cx.listener(move |_this, _ev, _window, _cx| {
			let mut offset = scroll_handle.offset();
			offset.x = (offset.x - px(180.0)).max(px(0.0));
			scroll_handle.set_offset(point(offset.x, offset.y));
		})
	};
	let scroll_right = {
		let scroll_handle = scroll_handle.clone();
		cx.listener(move |_this, _ev, _window, _cx| {
			let mut offset = scroll_handle.offset();
			offset.x = offset.x + px(180.0);
			scroll_handle.set_offset(point(offset.x, offset.y));
		})
	};

	let mut strip = div()
		.id("tabs")
		.flex()
		.flex_row()
		.items_center()
		.flex_1()
		.min_w(px(0.0))
		.h(px(32.0))
		.bg(t.app_bg)
		.border_b_1()
		.border_color(t.border)
		.track_scroll(&scroll_handle)
		.overflow_x_scroll();

	for (idx, tab) in tabs.into_iter().enumerate() {
		let tid = tab.id;
		let title = tab.title.clone();
		let text_color = if tab.active { t.text } else { t.text_dim };
		let bg = if tab.active { t.panel_bg } else { t.panel_bg_2 };
		let pin_icon_path = if tab.pinned {
			"round-push-pin.svg"
		} else {
			"round-pin-off.svg"
		};
		let pin_icon = Icon::empty().path(pin_icon_path);
		let close_icon = Icon::empty().path("close.svg");

		let tid1 = tid.clone();
		let tid2 = tid.clone();
		let tid3 = tid.clone();
		let tid4 = tid.clone();
		let tid5 = tid.clone();
		let tid6 = tid.clone();

		strip = strip.child(
			div()
				.id(("tab", idx))
				.flex()
				.items_center()
				.gap_2()
				.px_3()
				.h(px(32.0))
				.bg(bg)
				.hover({
					let t = t.clone();
					move |d| d.bg(t.surface_hover_bg)
				})
				.cursor_pointer()
				.on_mouse_down(MouseButton::Left, {
					let drag_start_cb = drag_start_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(drag_start_cb)(this, tid1.clone(), window, cx);
					})
				})
				.on_mouse_down(MouseButton::Right, {
					let rename_cb = rename_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(rename_cb)(this, tid2.clone(), window, cx);
					})
				})
				.on_mouse_up(MouseButton::Left, {
					let drop_cb = drop_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(drop_cb)(this, tid3.clone(), window, cx);
					})
				})
				.on_click({
					let select_cb = select_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(select_cb)(this, tid4.clone(), window, cx);
					})
				})
				.child(
					div()
						.id(("tab-pin", idx))
						.text_color(t.text_muted)
						.text_sm()
						.px_1()
						.rounded_sm()
						.hover({
							let t = t.clone();
							move |d| d.bg(t.icon_button_hover_bg)
						})
						.cursor_pointer()
						.child(pin_icon)
						.on_click({
							let pin_cb = pin_cb.clone();
							cx.listener(move |this, _ev, window, cx| {
								(pin_cb)(this, tid5.clone(), window, cx);
							})
						}),
				)
				.child(div().text_color(text_color).text_sm().child(title))
				.child(
					div()
						.id(("tab-close", idx))
						.text_color(t.text_muted)
						.text_sm()
						.px_1()
						.rounded_sm()
						.hover({
							let t = t.clone();
							move |d| d.bg(t.icon_button_hover_bg)
						})
						.cursor_pointer()
						.child(close_icon)
						.on_click({
							let close_cb = close_cb.clone();
							cx.listener(move |this, _ev, window, cx| {
								(close_cb)(this, tid6.clone(), window, cx);
							})
						}),
				)
				.into_any_element(),
		);
	}

	div()
		.id("tab-strip")
		.flex()
		.items_center()
		.flex_none()
		.w_full()
		.h(px(32.0))
		.bg(t.panel_bg)
		.border_b_1()
		.border_color(t.border)
		.child(
			div()
				.id("tab-scroll-left")
				.flex()
				.items_center()
				.justify_center()
				.px_2()
				.h(px(32.0))
				.text_color(t.text_muted)
				.cursor_pointer()
				.hover({
					let t = t.clone();
					move |d| d.bg(t.surface_hover_bg)
				})
				.child(Icon::empty().path("chevron-left.svg"))
				.on_click(scroll_left),
		)
		.child(strip)
		.child(
			div()
				.id("tab-add")
				.flex()
				.items_center()
				.justify_center()
				.px_3()
				.h(px(32.0))
				.bg(t.panel_bg)
				.text_color(t.text_muted)
				.cursor_pointer()
				.hover({
					let t = t.clone();
					move |d| d.bg(t.surface_hover_bg)
				})
				.child(Icon::empty().path("plus.svg"))
				.on_click({
					let add_cb = add_cb.clone();
					cx.listener(move |this, _ev, window, cx| {
						(add_cb)(this, window, cx);
					})
				}),
		)
		.child(
			div()
				.id("tab-scroll-right")
				.flex()
				.items_center()
				.justify_center()
				.px_2()
				.h(px(32.0))
				.text_color(t.text_muted)
				.cursor_pointer()
				.hover({
					let t = t.clone();
					move |d| d.bg(t.surface_hover_bg)
				})
				.child(Icon::empty().path("chevron-right.svg"))
				.on_click(scroll_right),
		)
}
