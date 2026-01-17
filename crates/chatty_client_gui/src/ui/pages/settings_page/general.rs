#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{div, px};

use gpui_component::StyledExt;
use gpui_component::input::Input;
use gpui_component::select::Select;

use crate::ui::theme;

use super::SettingsPage;

pub(super) fn render(this: &SettingsPage, t: &theme::Theme, preview_t: &theme::Theme) -> gpui::Div {
	let settings_section = div()
		.flex()
		.flex_col()
		.gap_2()
		.child(
			div()
				.flex()
				.flex_row()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Theme:"))
				.child(
					Select::new(&this.theme_select)
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
				.flex_row()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Default Platform:"))
				.child(
					Select::new(&this.default_platform_select)
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
				.flex_row()
				.items_center()
				.gap_2()
				.child(div().w(px(160.0)).child("Max Log Items:"))
				.child(
					Input::new(&this.max_log_items_input)
						.bg(t.panel_bg)
						.text_color(t.text)
						.border_1()
						.border_color(t.border)
						.rounded_sm(),
				),
		);

	let preview_section = div()
		.bg(preview_t.panel_bg)
		.border_1()
		.border_color(preview_t.border)
		.p_2()
		.rounded_sm()
		.child(
			div()
				.text_color(preview_t.text)
				.child(format!("Sample chat message in {:?} theme", this.settings.theme)),
		)
		.child(
			div()
				.text_color(preview_t.text_dim)
				.text_sm()
				.child("This is how text looks with the current settings."),
		);

	div()
		.flex()
		.flex_col()
		.gap_4()
		.child(div().text_sm().font_semibold().child("General"))
		.child(settings_section)
		.child(div().mt_6().child("Live Preview:").text_sm().font_semibold().mb_2())
		.child(preview_section)
}
