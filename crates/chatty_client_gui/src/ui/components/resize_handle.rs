#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{CursorStyle, MouseButton, MouseDownEvent, Window, div, px};

use crate::ui::theme;

pub fn render_resize_handle<T>(
	handle_index: usize,
	cx: &mut Context<T>,
	on_mouse_down: impl Fn(&mut T, &MouseDownEvent, &mut Window, &mut Context<T>) + 'static + Clone,
) -> impl IntoElement
where
	T: 'static,
{
	let t = theme::theme();
	div()
		.id(("split-handle", handle_index as u64))
		.w(px(4.0))
		.min_w(px(4.0))
		.flex_none()
		.h_full()
		.cursor(CursorStyle::ResizeLeftRight)
		.bg(t.border)
		.hover({
			let t = t.clone();
			move |d| d.bg(t.chat_nick)
		})
		.on_mouse_down(
			MouseButton::Left,
			cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
				on_mouse_down(this, ev, window, cx);
			}),
		)
}
