#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use gpui::prelude::*;
use gpui::{AnyElement, Entity, LineFragment, MouseButton, Pixels, Window, div, img, px};
use gpui_component::button::Button;

use crate::ui::app_state::{AppState, AssetRefUi, ChatItem, SystemNoticeUi, TabId};
use crate::ui::badges::cmp_badge_ids;
use crate::ui::theme;
use chatty_domain::RoomKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageContextInfo {
	pub tab_id: TabId,
	pub message_index: usize,
	pub room: RoomKey,
	pub author_login: String,
	pub author_id: Option<String>,
	pub server_message_id: Option<String>,
	pub platform_message_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageMenuAction {
	Reply,
	Delete,
	Timeout,
	Ban,
}

pub fn estimate_item_height<T>(item: &ChatItem, split_width: Pixels, window: &Window, cx: &mut Context<T>) -> Pixels
where
	T: 'static,
{
	let text = match item {
		ChatItem::ChatMessage(msg) => msg.text.as_str(),
		ChatItem::SystemNotice(SystemNoticeUi { text, .. }) => text.as_str(),
		ChatItem::Lagged(lagged) => lagged.detail.as_deref().unwrap_or("Messages skipped"),
	};
	let available_width = (f32::from(split_width) - 140.0).max(120.0);
	let font_size = window.rem_size();
	let mut line_wrapper = cx.text_system().line_wrapper(window.text_style().font(), font_size);
	let fragment = LineFragment::text(text);
	let fragments = [fragment];
	let boundaries = line_wrapper.wrap_line(&fragments, px(available_width));
	let line_count = boundaries.count().saturating_add(1).max(1) as f32;
	let base = font_size * 1.1;
	let line_height = font_size * 1.35;
	base + line_height * line_count
}

pub fn build_message_rows_range<T>(
	app_state: Entity<AppState>,
	active_tab: Option<TabId>,
	t: &theme::Theme,
	range: std::ops::Range<usize>,
	menu_state: Option<MessageContextInfo>,
	on_message_context: impl Fn(&mut T, MessageContextInfo, &mut Window, &mut Context<T>) + 'static + Clone,
	on_message_action: impl Fn(&mut T, MessageMenuAction, MessageContextInfo, &mut Window, &mut Context<T>) + 'static + Clone,
	on_clear_menu: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static + Clone,
	cx: &mut Context<T>,
) -> Vec<AnyElement>
where
	T: 'static,
{
	let mut rows: Vec<AnyElement> = Vec::new();
	let mut emote_lookup_cache: HashMap<RoomKey, HashMap<String, AssetRefUi>> = HashMap::new();
	let mut badge_lookup_cache: HashMap<RoomKey, HashMap<String, AssetRefUi>> = HashMap::new();

	let app = app_state.read(cx);
	if let Some(tab_id) = active_tab {
		if let Some(tab) = app.tabs.get(&tab_id) {
			let total = tab.log.items.len();
			let start = range.start.min(total);
			let end = range.end.min(total);
			for (offset, item) in tab.log.items.iter().skip(start).take(end - start).enumerate() {
				let idx = start + offset;
				match item {
					ChatItem::ChatMessage(msg) => {
						let context = MessageContextInfo {
							tab_id,
							message_index: idx,
							room: msg.room.clone(),
							author_login: msg.user_login.clone(),
							author_id: msg.author_id.clone(),
							server_message_id: msg.server_message_id.clone(),
							platform_message_id: msg.platform_message_id.clone(),
						};
						let perms = permissions_for_room(&app, &context.room);
						let emote_lookup = if let Some(lookup) = emote_lookup_cache.get(&context.room) {
							lookup
						} else {
							let lookup = build_emote_lookup(&app, &context.room);
							emote_lookup_cache.insert(context.room.clone(), lookup);
							emote_lookup_cache.get(&context.room).expect("emote lookup inserted")
						};
						let badge_lookup = if let Some(lookup) = badge_lookup_cache.get(&context.room) {
							lookup
						} else {
							let lookup = build_badge_lookup(&app, &context.room);
							badge_lookup_cache.insert(context.room.clone(), lookup);
							badge_lookup_cache.get(&context.room).expect("badge lookup inserted")
						};
						let has_reply_id = context.platform_message_id.is_some();
						let has_delete_id = context.platform_message_id.is_some();
						let has_author_id = context.author_id.is_some();
						let on_message_context = on_message_context.clone();
						let on_clear_menu = on_clear_menu.clone();
						let on_clear_menu_for_row = on_clear_menu.clone();
						let mut row = div()
							.id(("msg", idx as u64))
							.flex()
							.items_start()
							.gap_2()
							.px_3()
							.py_1()
							.bg(t.chat_row_bg)
							.hover({
								let t = t.clone();
								move |d| d.bg(t.chat_row_hover_bg)
							})
							.on_mouse_down(MouseButton::Right, {
								let context = context.clone();
								cx.listener(move |this, _ev, window, cx| {
									(on_message_context)(this, context.clone(), window, cx);
								})
							})
							.on_mouse_down(MouseButton::Left, {
								let on_clear_menu_for_row = on_clear_menu_for_row.clone();
								cx.listener(move |this, _ev, window, cx| {
									(on_clear_menu_for_row)(this, window, cx);
								})
							})
							.child(
								div()
									.text_sm()
									.text_color(t.text_muted)
									.flex_none()
									.child(format_time(msg.time)),
							)
							.child(render_badge_strip(&msg.badge_ids, &badge_lookup))
							.child(
								div()
									.text_sm()
									.text_color(t.chat_nick)
									.flex_none()
									.child(msg.user_display.clone().unwrap_or_else(|| msg.user_login.clone())),
							)
							.child(render_message_body(&msg.text, &emote_lookup, t));

						if perms.can_reply && has_reply_id || perms.can_delete && has_delete_id {
							let mut actions = div().flex().items_center().gap_1().flex_none();
							if perms.can_reply && has_reply_id {
								actions = actions.child(
									Button::new(("reply-inline", idx as u64))
										.px_2()
										.py_1()
										.rounded_sm()
										.bg(t.button_bg)
										.text_color(t.button_text)
										.text_sm()
										.label("Reply")
										.on_click(cx.listener({
											let context = context.clone();
											let on_message_action = on_message_action.clone();
											move |this, _ev, window, cx| {
												(on_message_action)(
													this,
													MessageMenuAction::Reply,
													context.clone(),
													window,
													cx,
												);
											}
										})),
								);
							}
							if perms.can_delete && has_delete_id {
								actions = actions.child(
									Button::new(("delete-inline", idx as u64))
										.px_2()
										.py_1()
										.rounded_sm()
										.bg(t.button_bg)
										.text_color(t.button_text)
										.text_sm()
										.label("Delete")
										.on_click(cx.listener({
											let context = context.clone();
											let on_message_action = on_message_action.clone();
											move |this, _ev, window, cx| {
												(on_message_action)(
													this,
													MessageMenuAction::Delete,
													context.clone(),
													window,
													cx,
												);
											}
										})),
								);
							}
							row = row.child(actions);
						}

						rows.push(row.into_any_element());

						if let Some(menu) = menu_state.as_ref() {
							if menu.tab_id == tab_id && menu.message_index == idx {
								let on_message_action = on_message_action.clone();
								let on_clear_menu = on_clear_menu.clone();

								let mut menu_row = div()
									.id(("msg-menu", idx as u64))
									.flex()
									.items_center()
									.gap_2()
									.px_3()
									.py_1()
									.bg(t.panel_bg_2)
									.border_1()
									.border_color(t.border)
									.rounded_sm();

								if perms.can_reply && has_reply_id {
									menu_row = menu_row.child(
										Button::new(("reply", idx as u64))
											.px_2()
											.py_1()
											.rounded_sm()
											.bg(t.button_bg)
											.text_color(t.button_text)
											.text_sm()
											.label("Reply")
											.on_click(cx.listener({
												let context = context.clone();
												let on_message_action = on_message_action.clone();
												move |this, _ev, window, cx| {
													(on_message_action)(
														this,
														MessageMenuAction::Reply,
														context.clone(),
														window,
														cx,
													);
												}
											})),
									);
								}

								if perms.can_delete && has_delete_id {
									menu_row = menu_row.child(
										Button::new(("delete", idx as u64))
											.px_2()
											.py_1()
											.rounded_sm()
											.bg(t.button_bg)
											.text_color(t.button_text)
											.text_sm()
											.label("Delete")
											.on_click(cx.listener({
												let context = context.clone();
												let on_message_action = on_message_action.clone();
												move |this, _ev, window, cx| {
													(on_message_action)(
														this,
														MessageMenuAction::Delete,
														context.clone(),
														window,
														cx,
													);
												}
											})),
									);
								}

								if perms.can_timeout && has_author_id {
									menu_row = menu_row.child(
										Button::new(("timeout", idx as u64))
											.px_2()
											.py_1()
											.rounded_sm()
											.bg(t.button_bg)
											.text_color(t.button_text)
											.text_sm()
											.label("Timeout")
											.on_click(cx.listener({
												let context = context.clone();
												let on_message_action = on_message_action.clone();
												move |this, _ev, window, cx| {
													(on_message_action)(
														this,
														MessageMenuAction::Timeout,
														context.clone(),
														window,
														cx,
													);
												}
											})),
									);
								}

								if perms.can_ban && has_author_id {
									menu_row = menu_row.child(
										Button::new(("ban", idx as u64))
											.px_2()
											.py_1()
											.rounded_sm()
											.bg(t.button_bg)
											.text_color(t.button_text)
											.text_sm()
											.label("Ban")
											.on_click(cx.listener({
												let context = context.clone();
												let on_message_action = on_message_action.clone();
												move |this, _ev, window, cx| {
													(on_message_action)(
														this,
														MessageMenuAction::Ban,
														context.clone(),
														window,
														cx,
													);
												}
											})),
									);
								}

								let mut id_parts = Vec::new();
								if let Some(id) = context.platform_message_id.as_deref() {
									id_parts.push(format!("msg:{id}"));
								}
								if let Some(id) = context.author_id.as_deref() {
									id_parts.push(format!("user:{id}"));
								}
								if let Some(id) = context.server_message_id.as_deref() {
									id_parts.push(format!("srv:{id}"));
								}
								if !id_parts.is_empty() {
									menu_row =
										menu_row.child(div().text_sm().text_color(t.text_muted).child(id_parts.join(" ")));
								}

								menu_row = menu_row.child(
									Button::new(("close", idx as u64))
										.px_2()
										.py_1()
										.rounded_sm()
										.bg(t.button_bg)
										.text_color(t.button_text)
										.text_sm()
										.label("Close")
										.on_click(cx.listener({
											let on_clear_menu = on_clear_menu.clone();
											move |this, _ev, window, cx| {
												(on_clear_menu)(this, window, cx);
											}
										})),
								);

								rows.push(menu_row.into_any_element());
							}
						}
					}
					ChatItem::SystemNotice(SystemNoticeUi { time, text }) => {
						rows.push(
							div()
								.id(("sys", idx as u64))
								.flex()
								.items_start()
								.gap_2()
								.px_3()
								.py_1()
								.bg(t.chat_row_bg)
								.child(div().text_sm().text_color(t.text_muted).flex_none().child(format_time(*time)))
								.child(
									div()
										.text_sm()
										.text_color(t.text_dim)
										.flex_1()
										.min_w(px(0.0))
										.child(text.clone()),
								)
								.into_any_element(),
						);
					}
					ChatItem::Lagged(lagged) => {
						rows.push(
							div()
								.id(("lagged", idx as u64))
								.flex()
								.items_start()
								.gap_2()
								.px_3()
								.py_1()
								.bg(t.chat_row_bg)
								.child(
									div()
										.text_sm()
										.text_color(t.text_muted)
										.flex_none()
										.child(format_time(lagged.time)),
								)
								.child(div().text_sm().text_color(t.text_dim).flex_1().min_w(px(0.0)).child(format!(
									"Messages skipped ({}): {}",
									lagged.dropped,
									lagged.detail.clone().unwrap_or_default()
								)))
								.into_any_element(),
						);
					}
				}
			}
		}
	}

	rows
}

fn permissions_for_room(app: &AppState, room: &RoomKey) -> crate::ui::app_state::RoomPermissions {
	app.room_permissions.get(room).copied().unwrap_or_default()
}

fn render_message_body(text: &str, emotes: &HashMap<String, AssetRefUi>, t: &theme::Theme) -> gpui::Div {
	let fragments = split_message_fragments(text, emotes);
	div()
		.text_sm()
		.text_color(t.text)
		.flex()
		.flex_wrap()
		.items_center()
		.gap_1()
		.flex_1()
		.min_w(px(0.0))
		.children(fragments)
}

fn render_badge_strip(badge_ids: &[String], badge_lookup: &HashMap<String, AssetRefUi>) -> gpui::Div {
	let mut seen = HashSet::new();
	let mut ordered: Vec<&String> = Vec::new();
	for id in badge_ids {
		if seen.insert(id.as_str()) {
			ordered.push(id);
		}
	}
	ordered.sort_by(|a, b| cmp_badge_ids(a, b));

	let badges = ordered
		.into_iter()
		.filter_map(|id| badge_lookup.get(id))
		.take(4)
		.map(|badge| img(badge.image_url.clone()).size_16().into_any_element())
		.collect::<Vec<_>>();

	if badges.is_empty() {
		return div();
	}

	div().flex().items_center().gap_1().flex_none().children(badges)
}

fn split_message_fragments(text: &str, emotes: &HashMap<String, AssetRefUi>) -> Vec<AnyElement> {
	let mut out = Vec::new();
	for token in split_with_whitespace(text) {
		if token.chars().all(|c| c.is_whitespace()) {
			out.push(div().child(token).into_any_element());
			continue;
		}

		if let Some((prefix, core, suffix)) = split_emote_token(&token) {
			if let Some(emote) = emotes.get(&core) {
				if !prefix.is_empty() {
					out.push(div().child(prefix).into_any_element());
				}
				out.push(img(emote.image_url.clone()).size_20().into_any_element());
				if !suffix.is_empty() {
					out.push(div().child(suffix).into_any_element());
				}
				continue;
			}
		}

		out.push(div().child(token).into_any_element());
	}
	out
}

fn split_with_whitespace(text: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut buf = String::new();
	let mut last_ws: Option<bool> = None;

	for ch in text.chars() {
		let ws = ch.is_whitespace();
		if let Some(prev) = last_ws {
			if prev != ws {
				if !buf.is_empty() {
					out.push(std::mem::take(&mut buf));
				}
			}
		}

		buf.push(ch);
		last_ws = Some(ws);
	}

	if !buf.is_empty() {
		out.push(buf);
	}

	out
}

fn split_emote_token(token: &str) -> Option<(String, String, String)> {
	let mut start = None;
	let mut end = None;
	for (idx, ch) in token.char_indices() {
		if is_emote_char(ch) {
			start.get_or_insert(idx);
			end = Some(idx + ch.len_utf8());
		}
	}

	let start = start?;
	let end = end?;
	let prefix = token[..start].to_string();
	let core = token[start..end].to_string();
	let suffix = token[end..].to_string();
	Some((prefix, core, suffix))
}

fn is_emote_char(ch: char) -> bool {
	ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-'
}

fn build_emote_lookup(app: &AppState, room: &RoomKey) -> HashMap<String, AssetRefUi> {
	let mut lookup = HashMap::new();

	for key in &app.global_asset_cache_keys {
		if let Some(bundle) = app.asset_bundles.get(key) {
			for emote in &bundle.emotes {
				lookup.entry(emote.name.clone()).or_insert_with(|| emote.clone());
			}
		}
	}

	if let Some(keys) = app.room_asset_cache_keys.get(room) {
		for key in keys {
			if let Some(bundle) = app.asset_bundles.get(key) {
				for emote in &bundle.emotes {
					lookup.insert(emote.name.clone(), emote.clone());
				}
			}
		}
	}

	lookup
}

fn build_badge_lookup(app: &AppState, room: &RoomKey) -> HashMap<String, AssetRefUi> {
	let mut lookup = HashMap::new();

	for key in &app.global_asset_cache_keys {
		if let Some(bundle) = app.asset_bundles.get(key) {
			for badge in &bundle.badges {
				lookup.entry(badge.id.clone()).or_insert_with(|| badge.clone());
			}
		}
	}

	if let Some(keys) = app.room_asset_cache_keys.get(room) {
		for key in keys {
			if let Some(bundle) = app.asset_bundles.get(key) {
				for badge in &bundle.badges {
					if !lookup.contains_key(&badge.id) {
						lookup.insert(badge.id.clone(), badge.clone());
					}
				}
			}
		}
	}

	lookup
}

fn format_time(ts: SystemTime) -> String {
	let ms = ts.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
	let s = (ms / 1000) % 60;
	let m = (ms / 1000 / 60) % 60;
	format!("{m:02}:{s:02}")
}
