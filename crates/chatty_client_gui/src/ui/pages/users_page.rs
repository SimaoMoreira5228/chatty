#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::collections::HashMap;

use gpui::prelude::*;
use gpui::{Window, div, px};
use gpui_component::scroll::ScrollableElement;

use crate::ui::app_state::{AppState, ChatItem, WindowId};
use crate::ui::theme;

pub struct UsersPage {
	app_state: gpui::Entity<AppState>,
	bound_window: WindowId,
}

impl UsersPage {
	pub fn new(
		_window: &mut Window,
		_cx: &mut Context<Self>,
		app_state: gpui::Entity<AppState>,
		bound_window: WindowId,
	) -> Self {
		Self { app_state, bound_window }
	}
}

impl Render for UsersPage {
	fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let t = theme::theme();

		let app = self.app_state.read(cx);
		let mut title = "Users".to_string();
		let mut user_rows: Vec<gpui::AnyElement> = Vec::new();
		if let Some(tab_id) = app.windows.get(&self.bound_window).and_then(|w| w.active_tab) {
			if let Some(tab) = app.tabs.get(&tab_id) {
				title = format!("Users • {}", tab.title);
				let mut counts: HashMap<String, usize> = HashMap::new();
				for item in tab.log.items.iter() {
					if let ChatItem::ChatMessage(msg) = item {
						*counts.entry(msg.user_login.clone()).or_insert(0) += 1;
					}
				}
				let mut users: Vec<(String, usize)> = counts.into_iter().collect();
				users.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
				if users.is_empty() {
					user_rows.push(
						div()
							.px_3()
							.py_2()
							.text_sm()
							.text_color(t.text_dim)
							.child("No users yet")
							.into_any_element(),
					);
				} else {
					for (idx, (user, count)) in users.into_iter().enumerate() {
						user_rows.push(
							div()
								.id(("user", idx as u64))
								.px_3()
								.py_2()
								.flex()
								.items_center()
								.justify_between()
								.text_color(t.text)
								.text_sm()
								.child(user)
								.child(div().text_xs().text_color(t.text_dim).child(format!("{count} messages")))
								.into_any_element(),
						);
					}
				}
			}
		}

		if user_rows.is_empty() {
			user_rows.push(
				div()
					.px_3()
					.py_2()
					.text_sm()
					.text_color(t.text_dim)
					.child("No active tab")
					.into_any_element(),
			);
		}

		let list = div().flex().flex_col().children(user_rows);

		div()
			.id("users-page")
			.size_full()
			.bg(t.panel_bg)
			.border_1()
			.border_color(t.border)
			.flex()
			.flex_col()
			.child(div().px_3().py_2().text_sm().text_color(t.text_dim).child(title))
			.child(div().flex_1().min_h(px(0.0)).overflow_y_scrollbar().child(list))
	}
}
