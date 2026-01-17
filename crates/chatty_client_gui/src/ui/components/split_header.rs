#![forbid(unsafe_code)]

use std::rc::Rc;

use gpui::AnyElement;
use gpui::MouseButton;
use gpui::prelude::*;
use gpui::{Entity, RetainAllImageCache, Window, div, image_cache, img, px};
use gpui_component::Icon;

use crate::ui::app_state::AssetRefUi;
use crate::ui::theme;

pub fn render_split_header<T, F, G, H, I>(
	title: String,
	active: bool,
	badges: Vec<AssetRefUi>,
	badge_cache: Entity<RetainAllImageCache>,
	can_close: bool,
	cx: &mut Context<T>,
	on_select: F,
	on_popout: G,
	on_close: H,
	on_context_menu: I,
) -> AnyElement
where
	F: Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
	G: Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
	H: Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
	I: Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
	T: 'static,
{
	let t = theme::theme();
	let select_cb = Rc::new(on_select);
	let popout_cb = Rc::new(on_popout);
	let close_cb = Rc::new(on_close);
	let context_cb = Rc::new(on_context_menu);
	let header_bg = if active { t.panel_bg_2 } else { t.panel_bg };

	let mut header_actions = div().flex().items_center().gap_2().child(
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
	);

	if can_close {
		header_actions = header_actions.child(
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
		);
	}

	let badge_strip = if badges.is_empty() {
		div().into_any_element()
	} else {
		image_cache(badge_cache)
			.child(
				div()
					.flex()
					.items_center()
					.gap_1()
					.children(badges.into_iter().take(3).map(|badge| img(badge.image_url).size_20())),
			)
			.into_any_element()
	};

	div()
		.id("chat-header")
		.h(px(28.0))
		.px_3()
		.flex()
		.items_center()
		.justify_between()
		.bg(header_bg)
		.border_b_1()
		.border_color(t.border)
		.cursor_pointer()
		.on_click({
			let select_cb = select_cb.clone();
			cx.listener(move |this, _ev, window, cx| {
				(select_cb)(this, window, cx);
			})
		})
		.on_mouse_down(MouseButton::Right, {
			let context_cb = context_cb.clone();
			cx.listener(move |this, _ev, window, cx| {
				(context_cb)(this, window, cx);
			})
		})
		.child(
			div()
				.flex()
				.items_center()
				.gap_2()
				.child(div().text_sm().text_color(t.chat_nick).child(title))
				.child(badge_strip),
		)
		.child(header_actions)
		.into_any_element()
}
