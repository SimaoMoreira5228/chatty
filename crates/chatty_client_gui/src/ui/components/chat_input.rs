#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{Entity, div, px};
use gpui_component::input::{Input, InputState};

use crate::ui::theme::Theme;

pub fn render_chat_input(input_state: Entity<InputState>, split_index: usize, t: &Theme) -> impl IntoElement {
	let input_box = div()
		.flex_1()
		.h(px(28.0))
		.px_2()
		.rounded_sm()
		.bg(t.panel_bg)
		.border_1()
		.border_color(t.border)
		.text_color(t.text)
		.text_sm()
		.child(Input::new(&input_state).appearance(false).text_color(t.text));

	let row = div().flex().items_center().child(input_box);

	div()
		.id(("chat-input", split_index as u64))
		.h(px(40.0))
		.px_3()
		.py_2()
		.bg(t.panel_bg_2)
		.border_t_1()
		.border_color(t.border)
		.child(row)
}
