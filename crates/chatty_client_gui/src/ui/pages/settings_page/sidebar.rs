#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{Window, div, px};

use gpui_component::StyledExt;
use gpui_component::button::Button;

use crate::ui::theme;

use super::{SettingsCategory, SettingsPage};

pub(super) fn render(
	this: &SettingsPage,
	t: &theme::Theme,
	_window: &mut Window,
	cx: &mut Context<SettingsPage>,
) -> impl IntoElement {
	let categories = SettingsCategory::all().into_iter().map(|category| {
		let selected = this.selected_category == category;
		let bg = if selected { t.panel_bg_2 } else { t.panel_bg };
		let text = if selected { t.text } else { t.text_dim };
		Button::new(("settings-category", category.key()))
			.w_full()
			.px_3()
			.py_2()
			.rounded_sm()
			.bg(bg)
			.text_color(text)
			.text_sm()
			.label(category.label())
			.on_click(cx.listener(move |this, _ev, _window, cx| {
				this.selected_category = category;
				cx.notify();
			}))
	});

	div()
		.id("settings-sidebar")
		.w(px(200.0))
		.flex()
		.flex_col()
		.gap_2()
		.p_3()
		.bg(t.panel_bg)
		.border_r_1()
		.border_color(t.border)
		.child(div().text_sm().font_semibold().child("Categories"))
		.children(categories)
}
