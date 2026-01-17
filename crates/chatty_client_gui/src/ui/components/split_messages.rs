#![forbid(unsafe_code)]

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Entity, Pixels, RetainAllImageCache, Window, div, image_cache, px, size};
use gpui_component::button::Button;
use gpui_component::v_virtual_list;

use crate::ui::app_state::{AppState, TabId};
use crate::ui::components::message_list::{
	MessageContextInfo, MessageMenuAction, build_message_rows_range, estimate_item_height,
};
use crate::ui::theme;

pub fn render_split_messages<T>(
	view: Entity<T>,
	app_state: Entity<AppState>,
	index: usize,
	has_tab: bool,
	item_count: usize,
	split_width: Pixels,
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
	if !has_tab {
		let on_create_tab = on_create_tab.clone();
		return div()
			.id(("chat-messages", index as u64))
			.flex()
			.flex_col()
			.items_center()
			.justify_center()
			.flex_1()
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
		return div()
			.id(("chat-messages", index as u64))
			.flex()
			.flex_col()
			.flex_1()
			.min_h(px(0.0))
			.bg(t.chat_bg)
			.child(div().px_3().py_2().text_sm().text_color(t.text_dim).child("No messages yet"))
			.into_any_element();
	}

	let items = {
		let app = app_state.read(cx);
		app.tabs
			.get(&tab_id)
			.map(|tab| tab.log.items.iter().cloned().collect::<Vec<_>>())
			.unwrap_or_default()
	};
	let item_sizes = Rc::new(
		items
			.iter()
			.map(|item| size(px(0.0), estimate_item_height(item, split_width, window, cx)))
			.collect::<Vec<_>>(),
	);
	let t_clone = t.clone();
	let tab_id_for_list = tab_id;
	let app_state_for_list = app_state.clone();
	let menu_state_for_list = message_menu.clone();
	let on_message_context = on_message_context.clone();
	let on_message_action = on_message_action.clone();
	let on_clear_menu = on_clear_menu.clone();
	let list = v_virtual_list(
		view,
		("chat-messages", index as u64),
		item_sizes,
		move |_this, range, _window, cx| {
			build_message_rows_range(
				app_state_for_list.clone(),
				Some(tab_id_for_list),
				&t_clone,
				range,
				menu_state_for_list.clone(),
				on_message_context.clone(),
				on_message_action.clone(),
				on_clear_menu.clone(),
				cx,
			)
		},
	);

	div()
		.flex()
		.flex_1()
		.w_full()
		.min_h(px(0.0))
		.child(image_cache(emote_image_cache).child(div().flex().flex_col().size_full().bg(t.chat_bg).child(list)))
		.into_any_element()
}
