#![forbid(unsafe_code)]

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Entity, Window, div};
use gpui_component::WindowExt;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputState};

use crate::ui::theme;

pub fn open_rename_dialog<T, F>(view: Entity<T>, window: &mut Window, cx: &mut Context<T>, current: String, on_submit: F)
where
	T: 'static,
	F: Fn(&mut T, &mut Context<T>, String) -> bool + 'static,
{
	let on_submit = Arc::new(on_submit);
	let input = cx.new(|cx| {
		let input = InputState::new(window, cx)
			.placeholder("Layout name")
			.validate(|s, _| !s.trim().is_empty());
		input
	});
	input.update(cx, |state, cx| {
		state.set_value(current, window, cx);
	});
	let t = theme::theme();

	window.open_dialog(cx, move |dialog, _window, _cx| {
		dialog
			.title("Rename layout")
			.confirm()
			.button_props(DialogButtonProps::default().ok_text("Save").cancel_text("Cancel"))
			.child(
				div()
					.flex()
					.flex_col()
					.gap_2()
					.child("Layout name")
					.child(Input::new(&input))
					.child(div().text_sm().text_color(t.text_dim).child("Give this layout a name.")),
			)
			.on_ok({
				let input = input.clone();
				let view = view.clone();
				let on_submit = on_submit.clone();
				move |_, _window, cx| {
					let raw = input.read(cx).value().to_string();
					view.update(cx, |this, cx| (on_submit)(this, cx, raw))
				}
			})
	})
}
