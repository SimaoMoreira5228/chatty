#![forbid(unsafe_code)]

use gpui::prelude::*;
use gpui::{AnyElement, Entity, Pixels, RetainAllImageCache, ScrollHandle, Window, div, image_cache, px};
use gpui_component::button::Button;
use gpui_component::scroll::ScrollableElement;

use crate::ui::app_state::{AppState, TabId};
use crate::ui::components::message_list::{MessageContextInfo, MessageMenuAction, build_message_rows_range};
use crate::ui::theme;
use tracing::debug;

#[allow(clippy::too_many_arguments)]
pub fn render_split_messages<T>(
	view: Entity<T>,
	app_state: Entity<AppState>,
	index: usize,
	has_tab: bool,
	item_count: usize,
	split_width: Pixels,
	split_height: Pixels,
	tab_id: TabId,
	emote_image_cache: Entity<RetainAllImageCache>,
	message_menu: Option<MessageContextInfo>,
	t: &theme::Theme,
	window: &mut Window,
	cx: &mut Context<T>,
	on_create_tab: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static + Clone,
	on_message_context: impl Fn(&mut T, MessageContextInfo, &mut Window, &mut Context<T>) + 'static + Clone,
	on_message_action: impl Fn(&mut T, MessageMenuAction, MessageContextInfo, &mut Window, &mut Context<T>) + 'static + Clone,
	on_clear_menu: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static + Clone,
) -> AnyElement
where
	T: Render + 'static,
{
	let _ = (view, split_width);
	debug!(
		tab_id = tab_id.0,
		has_tab,
		item_count,
		split_width = f32::from(split_width),
		split_height = f32::from(split_height),
		"render_split_messages inputs"
	);
	if !has_tab {
		let on_create_tab = on_create_tab.clone();
		return div()
			.id(("chat-messages", index as u64))
			.flex()
			.flex_col()
			.items_center()
			.justify_center()
			.flex_1()
			.h(split_height)
			.min_h(px(0.0))
			.bg(t.chat_bg)
			.child(div().px_3().py_2().text_sm().text_color(t.text_dim).child("No chat tabs yet"))
			.child(
				Button::new(("create-chat", index as u64))
					.px_3()
					.py_1()
					.rounded_sm()
					.bg(t.button_bg)
					.text_color(t.button_text)
					.text_sm()
					.label("Create chat tab")
					.on_click(cx.listener(move |this, _ev, window, cx| {
						on_create_tab(this, window, cx);
					})),
			)
			.into_any_element();
	}

	if item_count == 0 {
		debug!(tab_id = tab_id.0, "render split messages: empty tab");
		return div()
			.id(("chat-messages", index as u64))
			.flex()
			.flex_col()
			.flex_1()
			.h(split_height)
			.min_h(px(0.0))
			.bg(t.chat_bg)
			.child(div().px_3().py_2().text_sm().text_color(t.text_dim).child("No messages yet"))
			.into_any_element();
	}

	debug!(tab_id = tab_id.0, item_count, "render split messages");
	let t_clone = t.clone();
	let tab_id_for_list = tab_id;
	let app_state_for_list = app_state.clone();
	let menu_state_for_list = message_menu.clone();
	let on_message_context = on_message_context.clone();
	let on_message_action = on_message_action.clone();
	let on_clear_menu = on_clear_menu.clone();
	let list = div().flex().flex_col().items_stretch().children(build_message_rows_range(
		app_state_for_list,
		Some(tab_id_for_list),
		&t_clone,
		0..item_count,
		menu_state_for_list,
		on_message_context,
		on_message_action,
		on_clear_menu,
		cx,
	));
	let list = image_cache(emote_image_cache).child(list);
	let scroll_key = ((index as u64) << 32) | (tab_id.0 as u64 & 0xFFFF_FFFF);
	let scroll_handle = window
		.use_keyed_state(("messages-scroll", scroll_key), cx, |_, _| ScrollHandle::default())
		.read(cx)
		.clone();

	let list_container = div()
		.id(("chat-messages-scroll", index as u64))
		.flex_1()
		.h(split_height)
		.min_h(px(0.0))
		.bg(t.chat_bg)
		.track_scroll(&scroll_handle)
		.overflow_y_scroll()
		.vertical_scrollbar(&scroll_handle)
		.child(list);

	div()
		.flex()
		.flex_1()
		.h(split_height)
		.w_full()
		.min_h(px(0.0))
		.child(
			div()
				.flex()
				.h(split_height)
				.flex_col()
				.flex_1()
				.w_full()
				.min_h(px(0.0))
				.child(list_container),
		)
		.into_any_element()
}
