#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::Entity;
use gpui::prelude::*;
use gpui::{
	Bounds, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, RetainAllImageCache, ScrollHandle, Window,
	WindowBounds, WindowKind, WindowOptions, div, px, size,
};
use gpui_component::Root;
use gpui_component::WindowExt;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::notification::Notification;

use crate::ui::app_state::{AppState, AssetRefUi, ChatMessageUi, TabId, TabTarget, WindowId};
use crate::ui::badges::cmp_badge_ids;
use crate::ui::components::message_list::{MessageContextInfo, MessageMenuAction};
use crate::ui::components::{
	ResizeDrag, StatusChip, TabItem, TopbarButton, begin_resize_drag, default_min_split_width, end_resize_drag,
	ensure_split_proportions, open_join_dialog, open_rename_dialog, rebalance_split_proportions, render_chat_input,
	render_resize_handle, render_split_controls, render_split_header, render_split_messages, render_tab_strip,
	render_topbar, split_content_width, update_resize_drag,
};
use crate::ui::net::NetController;
use crate::ui::settings;
use crate::ui::theme;
use chatty_domain::{RoomId, RoomKey, RoomTopic};
use chatty_protocol::pb;

pub struct MainPage {
	pub app_state: Entity<AppState>,
	pub bound_window: WindowId,
	pub net_controller: Option<NetController>,
	emote_image_cache: Entity<RetainAllImageCache>,

	layouts: Vec<LayoutModel>,
	active_layout_id: Option<String>,
	window_id: String,

	resize_drag: Option<ResizeDrag>,
	split_bounds: Option<Bounds<Pixels>>,
	dragging_tab: Option<TabId>,
	dragging_tab_str: Option<String>,
	message_menu: Option<MessageContextInfo>,
	is_primary: bool,
}

#[derive(Debug, Clone)]
struct SplitPanel {
	id: String,
	tabs: Vec<TabId>,
	active_tab_id: Option<TabId>,
	input: Option<Entity<InputState>>,
	draft: String,
}

#[derive(Debug, Clone)]
struct LayoutModel {
	id: String,
	title: String,
	pinned: bool,
	splits: Vec<SplitPanel>,
	split_proportions: Vec<f32>,
	active_split_index: usize,
	use_fixed_split_widths: bool,
	desired_split_count: usize,
}

impl SplitPanel {
	fn new(id: String, tab_id: TabId) -> Self {
		Self {
			id,
			tabs: vec![tab_id],
			active_tab_id: Some(tab_id),
			input: None,
			draft: String::new(),
		}
	}

	fn empty(id: String) -> Self {
		Self {
			id,
			tabs: Vec::new(),
			active_tab_id: None,
			input: None,
			draft: String::new(),
		}
	}
}

impl MainPage {
	#[allow(clippy::too_many_arguments)]
	pub fn new(
		app_state: Entity<AppState>,
		bound_window: WindowId,
		net_controller: Option<NetController>,
		emote_image_cache: Entity<RetainAllImageCache>,
		window_id: String,
		layout: settings::UiWindow,
		is_primary: bool,
		cx: &mut Context<Self>,
	) -> Self {
		let mut page = Self {
			app_state,
			bound_window,
			net_controller,
			emote_image_cache,
			layouts: Vec::new(),
			active_layout_id: None,
			window_id,
			resize_drag: None,
			split_bounds: None,
			dragging_tab: None,
			dragging_tab_str: None,
			message_menu: None,
			is_primary,
		};

		page.bootstrap_from_settings(layout, cx);
		page
	}

	fn bootstrap_from_settings(&mut self, state: settings::UiWindow, cx: &mut Context<Self>) {
		if state.layouts.is_empty() {
			self.create_default_layout();
			return;
		}

		self.layouts = state
			.layouts
			.into_iter()
			.map(|l| {
				let active_split_id = l.active_split_id.clone();
				let total_width: f32 = l.splits.iter().map(|s| s.width).sum();
				let split_proportions = if total_width > 0.0 {
					l.splits.iter().map(|s| s.width / total_width).collect()
				} else {
					let count = l.splits.len();
					if count > 0 {
						vec![1.0 / count as f32; count]
					} else {
						Vec::new()
					}
				};

				let mut active_split_index = 0usize;
				if let Some(active_id) = active_split_id.as_ref() {
					if let Some(idx) = l.splits.iter().position(|s| &s.id == active_id) {
						active_split_index = idx;
					}
				}

				LayoutModel {
					id: l.id,
					title: l.title,
					pinned: l.pinned,
					active_split_index,
					use_fixed_split_widths: false,
					desired_split_count: l.splits.len(),
					split_proportions,
					splits: l
						.splits
						.into_iter()
						.map(|s| {
							let tabs = s
								.tabs
								.into_iter()
								.map(|t| self.restore_tab_to_app_state(t, cx))
								.collect::<Vec<_>>();

							SplitPanel {
								id: s.id,
								active_tab_id: s.active_tab_id.map(|id| TabId(id.parse().unwrap_or(0))),
								tabs,
								input: None,
								draft: String::new(),
							}
						})
						.collect(),
				}
			})
			.collect();

		self.active_layout_id = state.active_layout_id;
		if self.active_layout_id.is_none() && !self.layouts.is_empty() {
			self.active_layout_id = Some(self.layouts[0].id.clone());
		}

		let desired_active_tab = self
			.active_layout()
			.and_then(|l| l.splits.get(l.active_split_index))
			.and_then(|s| s.active_tab_id)
			.filter(|id| id.0 != 0);
		if let Some(tab_id) = desired_active_tab {
			let win = self.bound_window;
			self.app_state.update(cx, |state, _cx| {
				state.set_active_tab(win, tab_id);
			});
		}
	}

	fn restore_tab_to_app_state(&mut self, ut: settings::UiTab, cx: &mut Context<Self>) -> TabId {
		let win = self.bound_window;
		self.app_state.update(cx, |state, _cx| {
			let tab_id = state.restore_tab(ut);
			state.attach_tab_to_window(win, tab_id);
			tab_id
		})
	}

	fn create_default_layout(&mut self) {
		let layout_id = "layout-1".to_string();
		let split_id = "split-1".to_string();
		let tab_id = TabId(0); // Empty tab

		let split = SplitPanel::new(split_id, tab_id);
		let layout = LayoutModel {
			id: layout_id.clone(),
			title: "Layout 1".to_string(),
			pinned: false,
			splits: vec![split],
			split_proportions: vec![1.0],
			active_split_index: 0,
			use_fixed_split_widths: false,
			desired_split_count: 1,
		};

		self.layouts.push(layout);
		self.active_layout_id = Some(layout_id);
	}

	fn drain_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		let notifications = self.app_state.update(cx, |state, _cx| state.take_notifications());

		for note in notifications {
			let notification = match note.kind {
				crate::ui::app_state::UiNotificationKind::Info => Notification::info(note.message),
				crate::ui::app_state::UiNotificationKind::Success => Notification::success(note.message),
				crate::ui::app_state::UiNotificationKind::Warning => Notification::warning(note.message),
				crate::ui::app_state::UiNotificationKind::Error => Notification::error(note.message),
			};

			window.push_notification(notification, cx);
		}
	}

	pub fn persist_layout(&self, window: &mut Window, cx: &mut Context<Self>) {
		let app = self.app_state.read(cx);

		let win_state = settings::UiWindow {
			id: self.window_id.clone(),
			is_primary: self.is_primary,
			x: f32::from(window.bounds().origin.x),
			y: f32::from(window.bounds().origin.y),
			width: f32::from(window.bounds().size.width),
			height: f32::from(window.bounds().size.height),
			active_layout_id: self.active_layout_id.clone(),
			layouts: self
				.layouts
				.iter()
				.map(|l| settings::UiLayout {
					id: l.id.clone(),
					title: l.title.clone(),
					pinned: l.pinned,
					active_split_id: l.splits.get(l.active_split_index).map(|s| s.id.clone()),
					splits: l
						.splits
						.iter()
						.enumerate()
						.map(|(i, s)| settings::UiSplit {
							id: s.id.clone(),
							width: l.split_proportions.get(i).cloned().unwrap_or(0.0),
							active_tab_id: s.active_tab_id.map(|id| id.0.to_string()),
							tabs: s
								.tabs
								.iter()
								.filter_map(|tab_id| {
									app.tabs.get(tab_id).map(|tab| match &tab.target {
										TabTarget::Room(room) => settings::UiTab {
											id: tab_id.0.to_string(),
											title: tab.title.clone(),
											room: Some(room.clone()),
											group_id: None,
											pinned: tab.pinned,
										},
										TabTarget::Group(group_id) => settings::UiTab {
											id: tab_id.0.to_string(),
											title: tab.title.clone(),
											room: None,
											group_id: Some(group_id.0),
											pinned: tab.pinned,
										},
									})
								})
								.collect(),
						})
						.collect(),
				})
				.collect(),
		};

		let mut root = settings::load_ui_layout().unwrap_or_default();
		if let Some(existing) = root.windows.iter_mut().find(|w| w.id == self.window_id) {
			*existing = win_state;
		} else {
			root.windows.push(win_state);
		}

		settings::save_ui_layout(&root);
	}

	pub fn persist_ui_state(&self, cx: &mut Context<Self>) {
		let app = self.app_state.read(cx);
		let mut root = settings::load_ui_layout().unwrap_or_default();

		if let Some(win) = root.windows.iter_mut().find(|w| w.id == self.window_id) {
			win.active_layout_id = self.active_layout_id.clone();
			win.layouts = self
				.layouts
				.iter()
				.map(|l| settings::UiLayout {
					id: l.id.clone(),
					title: l.title.clone(),
					pinned: l.pinned,
					active_split_id: l.splits.get(l.active_split_index).map(|s| s.id.clone()),
					splits: l
						.splits
						.iter()
						.enumerate()
						.map(|(i, s)| settings::UiSplit {
							id: s.id.clone(),
							width: l.split_proportions.get(i).cloned().unwrap_or(0.0),
							active_tab_id: s.active_tab_id.map(|id| id.0.to_string()),
							tabs: s
								.tabs
								.iter()
								.filter_map(|tab_id| {
									app.tabs.get(tab_id).map(|tab| match &tab.target {
										TabTarget::Room(room) => settings::UiTab {
											id: tab_id.0.to_string(),
											title: tab.title.clone(),
											room: Some(room.clone()),
											group_id: None,
											pinned: tab.pinned,
										},
										TabTarget::Group(group_id) => settings::UiTab {
											id: tab_id.0.to_string(),
											title: tab.title.clone(),
											room: None,
											group_id: Some(group_id.0),
											pinned: tab.pinned,
										},
									})
								})
								.collect(),
						})
						.collect(),
				})
				.collect();

			settings::save_ui_layout(&root);
		}
	}

	fn active_layout_mut(&mut self) -> Option<&mut LayoutModel> {
		let id = self.active_layout_id.as_ref()?;
		self.layouts.iter_mut().find(|l| &l.id == id)
	}

	fn active_layout(&self) -> Option<&LayoutModel> {
		let id = self.active_layout_id.as_ref()?;
		self.layouts.iter().find(|l| &l.id == id)
	}

	fn select_layout_tab(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
		self.active_layout_id = Some(id);
		if let Some(layout) = self.active_layout()
			&& let Some(split) = layout.splits.get(layout.active_split_index)
			&& let Some(tab_id) = split.active_tab_id
		{
			let win = self.bound_window;
			self.app_state.update(cx, |state, _cx| {
				state.set_active_tab(win, tab_id);
			});
		}
		self.persist_layout(window, cx);
		cx.notify();
	}

	fn add_layout_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		let layout_id = format!(
			"layout-{}",
			SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
		);
		let title = format!("Layout {}", self.layouts.len() + 1);

		let split_id = format!("split-{}", layout_id);
		let tab_id = TabId(0);
		let split = SplitPanel::new(split_id, tab_id);

		let layout = LayoutModel {
			id: layout_id.clone(),
			title,
			pinned: false,
			splits: vec![split],
			split_proportions: vec![1.0],
			active_split_index: 0,
			use_fixed_split_widths: false,
			desired_split_count: 1,
		};

		self.layouts.push(layout);
		self.select_layout_tab(layout_id, window, cx);
		self.open_join_dialog(None, window, cx);
	}

	fn close_layout_tab(&mut self, layout_id: String, window: &mut Window, cx: &mut Context<Self>) {
		let Some(index) = self.layouts.iter().position(|l| l.id == layout_id) else {
			return;
		};

		self.layouts.remove(index);
		if self.active_layout_id.as_ref() == Some(&layout_id) {
			if self.layouts.is_empty() {
				self.active_layout_id = None;
			} else {
				let new_index = index.min(self.layouts.len() - 1);
				let new_id = self.layouts[new_index].id.clone();
				self.select_layout_tab(new_id, window, cx);
			}
		}
		self.persist_layout(window, cx);
		cx.notify();
	}

	fn move_layout_tab(&mut self, from_id: String, to_id: String, window: &mut Window, cx: &mut Context<Self>) {
		let Some(from_idx) = self.layouts.iter().position(|l| l.id == from_id) else {
			return;
		};
		let Some(to_idx) = self.layouts.iter().position(|l| l.id == to_id) else {
			return;
		};
		if from_idx == to_idx {
			return;
		}

		let layout = self.layouts.remove(from_idx);
		self.layouts.insert(to_idx, layout);

		self.persist_layout(window, cx);
		cx.notify();
	}

	fn toggle_pin_layout_tab(&mut self, layout_id: String, window: &mut Window, cx: &mut Context<Self>) {
		if let Some(l) = self.layouts.iter_mut().find(|l| l.id == layout_id) {
			l.pinned = !l.pinned;
		}

		let active_id = self.active_layout_id.clone();
		let mut pinned = Vec::new();
		let mut unpinned = Vec::new();
		for l in self.layouts.drain(..) {
			if l.pinned {
				pinned.push(l);
			} else {
				unpinned.push(l);
			}
		}

		self.layouts = pinned.into_iter().chain(unpinned).collect();
		self.active_layout_id = active_id;

		self.persist_layout(window, cx);
		cx.notify();
	}

	fn render_topbar(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let settings_bounds = Bounds::centered(None, size(px(760.0), px(540.0)), cx);
		let users_bounds = Bounds::centered(None, size(px(420.0), px(420.0)), cx);
		let status = self.app_state.read(cx).connection.clone();
		let t = theme::theme();

		let status_chip = match &status {
			crate::ui::app_state::ConnectionStatus::Connected { server } => Some(StatusChip {
				label: format!("Connected • {server}").into(),
				color: t.accent_green,
			}),
			crate::ui::app_state::ConnectionStatus::Connecting => Some(StatusChip {
				label: "Connecting".into(),
				color: t.text_muted,
			}),
			crate::ui::app_state::ConnectionStatus::Reconnecting {
				attempt,
				next_retry_in_ms,
			} => Some(StatusChip {
				label: format!("Reconnecting in {}s (#{})", next_retry_in_ms / 1000, attempt).into(),
				color: t.text_muted,
			}),
			crate::ui::app_state::ConnectionStatus::Disconnected { reason } => {
				let label = if let Some(reason) = reason {
					format!("Disconnected • {}", reason)
				} else {
					"Disconnected".to_string()
				};
				Some(StatusChip {
					label: label.into(),
					color: t.text_muted,
				})
			}
		};

		let connection_action: Option<TopbarButton<Self>> = self.net_controller.clone().map(|net| match status {
			crate::ui::app_state::ConnectionStatus::Disconnected { .. } => TopbarButton {
				id: "btn-connect",
				label: "Connect".into(),
				on_click: Arc::new(move |this: &mut Self, _window, cx| {
					let net = net.clone();
					let app_state = this.app_state.clone();
					cx.spawn(async move |_, cx| {
						let settings_snapshot = settings::get_cloned();
						let cfg = match settings::build_client_config(&settings_snapshot) {
							Ok(cfg) => cfg,
							Err(err) => {
								app_state.update(cx, |state, _cx| {
									state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
								});
								return;
							}
						};
						if let Err(err) = net.connect(cfg).await {
							app_state.update(cx, |state, _cx| {
								state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
							});
						}
					})
					.detach();
				}),
			},
			crate::ui::app_state::ConnectionStatus::Connecting => TopbarButton {
				id: "btn-disconnect",
				label: "Cancel".into(),
				on_click: Arc::new(move |this: &mut Self, _window, cx| {
					let net = net.clone();
					let app_state = this.app_state.clone();
					cx.spawn(async move |_, cx| {
						if let Err(err) = net.disconnect("user cancel").await {
							app_state.update(cx, |state, _cx| {
								state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
							});
						}
					})
					.detach();
				}),
			},
			crate::ui::app_state::ConnectionStatus::Reconnecting { .. } => TopbarButton {
				id: "btn-reconnect",
				label: "Reconnect now".into(),
				on_click: Arc::new(move |this: &mut Self, _window, cx| {
					let net = net.clone();
					let app_state = this.app_state.clone();
					cx.spawn(async move |_, cx| {
						let settings_snapshot = settings::get_cloned();
						let cfg = match settings::build_client_config(&settings_snapshot) {
							Ok(cfg) => cfg,
							Err(err) => {
								app_state.update(cx, |state, _cx| {
									state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
								});
								return;
							}
						};
						let _ = net.connect(cfg).await;
					})
					.detach();
				}),
			},
			crate::ui::app_state::ConnectionStatus::Connected { .. } => TopbarButton {
				id: "btn-disconnect",
				label: "Disconnect".into(),
				on_click: Arc::new(move |this: &mut Self, _window, cx| {
					let net = net.clone();
					let app_state = this.app_state.clone();
					cx.spawn(async move |_, cx| {
						if let Err(err) = net.disconnect("user disconnect").await {
							app_state.update(cx, |state, _cx| {
								state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
							});
						}
					})
					.detach();
				}),
			},
		});

		let app_state_for_users = self.app_state.clone();
		let bound_window_for_users = self.bound_window;
		let app_state_for_settings = self.app_state.clone();
		let bound_window_for_settings = self.bound_window;
		let net_for_settings = self.net_controller.clone();

		render_topbar(
			"Chatty",
			status_chip,
			_window,
			cx,
			move |_this: &mut Self, _window, cx| {
				let app_state = app_state_for_users.clone();
				let bound_window = bound_window_for_users;
				let _ = cx.open_window(
					WindowOptions {
						window_bounds: Some(WindowBounds::Windowed(users_bounds)),
						kind: WindowKind::Floating,
						..Default::default()
					},
					move |window, cx| {
						let view = cx.new(|cx| {
							crate::ui::pages::users_page::UsersPage::new(window, cx, app_state.clone(), bound_window)
						});
						cx.new(|cx| Root::new(view, window, cx))
					},
				);
				cx.notify();
			},
			move |_this: &mut Self, _window, cx| {
				let app_state = app_state_for_settings.clone();
				let bound_window = bound_window_for_settings;
				let net_controller = net_for_settings.clone();
				let _ = cx.open_window(
					WindowOptions {
						window_bounds: Some(WindowBounds::Windowed(settings_bounds)),
						kind: WindowKind::Floating,
						..Default::default()
					},
					move |window, cx| {
						let view = cx.new(|cx| {
							crate::ui::pages::settings_page::SettingsPage::new(
								window,
								cx,
								Some(app_state.clone()),
								Some(bound_window),
								net_controller.clone(),
							)
						});
						cx.new(|cx| Root::new(view, window, cx))
					},
				);
				cx.notify();
			},
			connection_action,
		)
	}

	fn render_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let scroll_handle = window
			.use_keyed_state("tab-strip-scroll", cx, |_, _| ScrollHandle::default())
			.read(cx)
			.clone();
		let tab_items: Vec<TabItem<String>> = self
			.layouts
			.iter()
			.map(|l| TabItem {
				id: l.id.clone(),
				title: l.title.clone(),
				active: self.active_layout_id.as_ref() == Some(&l.id),
				pinned: l.pinned,
			})
			.collect();

		render_tab_strip(
			tab_items,
			scroll_handle,
			window,
			cx,
			|this, id_str, window, cx| {
				this.select_layout_tab(id_str, window, cx);
			},
			|this, id_str, window, cx| {
				this.close_layout_tab(id_str, window, cx);
			},
			|this, window, cx| {
				this.add_layout_tab(window, cx);
			},
			|this, id_str, _window, _cx| {
				this.dragging_tab_str = Some(id_str);
			},
			|_this, _id_str, _window, _cx| {
				// No-op for now
			},
			|this, id_str, window, cx| {
				this.toggle_pin_layout_tab(id_str, window, cx);
			},
			|this, id_str, window, cx| {
				this.open_rename_layout_dialog(id_str, window, cx);
			},
		)
	}

	fn subscribe_rooms_for_tab(&self, tab_id: TabId, cx: &mut Context<Self>) {
		let rooms = self.app_state.read(cx).rooms_for_tab(tab_id);
		let Some(net) = self.net_controller.clone() else {
			return;
		};
		cx.spawn(async move |_, _cx| {
			for room in rooms {
				let _ = net.subscribe_room_key(room).await;
			}
		})
		.detach();
	}

	fn unsubscribe_rooms_if_unused(&self, rooms: Vec<RoomKey>, cx: &mut Context<Self>) {
		let Some(net) = self.net_controller.clone() else {
			return;
		};
		let app_ent = self.app_state.clone();
		cx.spawn(async move |_, cx| {
			for room in rooms {
				let still_used = app_ent.update(cx, |state, _cx| {
					state
						.tabs
						.keys()
						.any(|tab_id| state.rooms_for_tab(*tab_id).iter().any(|rk| rk == &room))
				});
				if !still_used {
					let _ = net.unsubscribe_room_key(room).await;
				}
			}
		})
		.detach();
	}

	fn render_main_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let t = theme::theme();

		if self.layouts.is_empty() {
			return div()
				.id("splits")
				.flex()
				.flex_col()
				.items_center()
				.justify_center()
				.size_full()
				.bg(t.app_bg)
				.child(div().text_color(t.text_dim).child("No layouts open"))
				.child(
					gpui_component::button::Button::new("create-layout")
						.label("Create Layout")
						.on_click(cx.listener(|this, _, window, cx| {
							this.add_layout_tab(window, cx);
						})),
				)
				.into_any_element();
		}

		self.drain_notifications(window, cx);
		self.ensure_splits(cx);

		self.drain_notifications(window, cx);
		self.ensure_splits(cx);

		let split_count = {
			let active_id = &self.active_layout_id;
			let layout = active_id
				.as_ref()
				.and_then(|id| self.layouts.iter_mut().find(|l| &l.id == id));

			if let Some(layout) = layout {
				let bounds = window.bounds();
				self.split_bounds = Some(bounds);

				crate::ui::components::split_sizing::ensure_split_proportions(
					&mut layout.split_proportions,
					layout.splits.len(),
				);
				layout.splits.len()
			} else {
				0
			}
		};

		let mut container = div()
			.id("splits")
			.flex()
			.flex_row()
			.flex_1()
			.min_h(px(0.0))
			.w_full()
			.bg(t.app_bg)
			.relative()
			.on_mouse_move(cx.listener(Self::on_resize_move))
			.on_mouse_up(MouseButton::Left, cx.listener(Self::on_resize_end))
			.on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_resize_end));

		for idx in 0..split_count {
			let split = self.render_split_panel(idx, window, cx);
			container = container.child(split);
			if idx + 1 < split_count {
				container = container.child(render_resize_handle(idx, cx, move |this, ev, _window, cx| {
					this.begin_resize(idx, ev, cx);
				}));
			}
		}

		container.into_any_element()
	}

	fn render_split_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		render_split_controls(
			window,
			cx,
			|this, window, cx| this.add_split(window, cx),
			|this, window, cx| this.popout_active_split(window, cx),
			|this, window, cx| this.remove_split(window, cx),
		)
	}

	fn open_join_dialog(&mut self, initial_value: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
		let view = cx.entity();
		let groups = self
			.app_state
			.read(cx)
			.groups
			.iter()
			.map(|(gid, g)| (gid.0, g.name.clone()))
			.collect();

		open_join_dialog(view, window, cx, groups, initial_value, |this, cx, submission| {
			this.apply_join_submission(submission, cx)
		});
	}

	fn apply_join_submission(
		&mut self,
		submission: crate::ui::components::join_dialog::JoinSubmission,
		cx: &mut Context<Self>,
	) -> bool {
		match submission {
			crate::ui::components::join_dialog::JoinSubmission::Single(raw) => self.apply_join_raw(&raw, cx),
			crate::ui::components::join_dialog::JoinSubmission::Group(raw) => self.apply_join_group(raw, cx),
		}
	}

	fn apply_join_group(&mut self, list: String, cx: &mut Context<Self>) -> bool {
		let default_platform = self.app_state.read(cx).default_platform;
		let mut rooms = Vec::new();
		for item in list.split(',') {
			let item = item.trim();
			if item.is_empty() {
				continue;
			}
			if let Ok(room) = chatty_domain::RoomKey::parse(item) {
				rooms.push(room);
				continue;
			}
			if let Ok(room_id) = chatty_domain::RoomId::new(item.to_string()) {
				rooms.push(chatty_domain::RoomKey::new(default_platform, room_id));
			}
		}

		if rooms.is_empty() {
			return false;
		}

		let win = self.bound_window;
		let tab_id = self.app_state.update(cx, |state, _cx| {
			let gid = state.create_group("New Group", rooms);
			let tab_id = state.create_tab_for_group("New Group", gid);
			state.add_tab_to_window(win, tab_id);
			tab_id
		});

		let idx = self.active_layout().unwrap().active_split_index;
		self.update_split_tab(idx, tab_id, cx);
		self.subscribe_rooms_for_tab(tab_id, cx);
		self.persist_ui_state(cx);
		cx.notify();
		true
	}

	fn apply_join_raw(&mut self, raw: &str, cx: &mut Context<Self>) -> bool {
		if let Some(rest) = raw.strip_prefix("group:")
			&& let Ok(gid) = rest.parse::<u64>()
		{
			let group_id = crate::ui::app_state::GroupId(gid);
			let win = self.bound_window;
			let tab_id = self.app_state.update(cx, |state, _cx| {
				let title = state
					.group(group_id)
					.map(|g| g.name.clone())
					.unwrap_or_else(|| "group".to_string());
				let tab_id = state.create_tab_for_group(title, group_id);
				state.add_tab_to_window(win, tab_id);
				tab_id
			});

			let idx = self.active_layout().unwrap().active_split_index;
			self.update_split_tab(idx, tab_id, cx);
			self.subscribe_rooms_for_tab(tab_id, cx);
			self.persist_ui_state(cx);
			cx.notify();
			return true;
		}

		let default_platform = self.app_state.read(cx).default_platform;
		let room = if let Ok(room) = chatty_domain::RoomKey::parse(raw) {
			room
		} else {
			match chatty_domain::RoomId::new(raw.to_string()) {
				Ok(id) => chatty_domain::RoomKey::new(default_platform, id),
				Err(_) => return false,
			}
		};

		let win = self.bound_window;
		let tab_id = self.app_state.update(cx, |state, _cx| {
			let tab_id = state.create_tab_for_room(room.room_id.to_string(), room);
			state.add_tab_to_window(win, tab_id);
			tab_id
		});

		let idx = self.active_layout().unwrap().active_split_index;
		self.update_split_tab(idx, tab_id, cx);
		self.subscribe_rooms_for_tab(tab_id, cx);
		self.persist_ui_state(cx);
		cx.notify();
		true
	}

	fn open_rename_layout_dialog(&mut self, layout_id: String, window: &mut Window, cx: &mut Context<Self>) {
		let Some(layout) = self.layouts.iter().find(|l| l.id == layout_id).cloned() else {
			return;
		};
		let view = cx.entity();
		open_rename_dialog(view, window, cx, layout.title.clone(), move |this, cx, raw| {
			let name = raw.trim();

			if name.is_empty() {
				return false;
			}

			if let Some(l) = this.layouts.iter_mut().find(|l| l.id == layout_id) {
				l.title = name.to_string();
			}

			this.persist_ui_state(cx);
			cx.notify();

			true
		});
	}

	fn render_footer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		div()
			.flex()
			.flex_col()
			.flex_none()
			.w_full()
			.child(self.render_split_controls(window, cx))
	}

	fn ensure_splits(&mut self, cx: &mut Context<Self>) {
		let (valid_tabs, window_tabs): (Vec<TabId>, Vec<TabId>) = {
			let app = self.app_state.read(cx);
			let valid_tabs = app.tabs.keys().copied().collect::<Vec<_>>();
			let window_tabs = app
				.windows
				.get(&self.bound_window)
				.map(|w| w.tabs.clone())
				.unwrap_or_default();
			(valid_tabs, window_tabs)
		};

		let layout = match self.active_layout_mut() {
			Some(l) => l,
			None => return,
		};
		let default_tab = window_tabs.first().copied();

		layout.splits.retain(|s| {
			s.active_tab_id
				.is_none_or(|id| id.0 == 0 || (valid_tabs.contains(&id) && window_tabs.contains(&id)))
		});

		for split in &mut layout.splits {
			split
				.tabs
				.retain(|id| id.0 == 0 || (valid_tabs.contains(id) && window_tabs.contains(id)));
			if let Some(active) = split.active_tab_id
				&& active.0 != 0
				&& valid_tabs.contains(&active)
				&& window_tabs.contains(&active)
				&& !split.tabs.contains(&active)
			{
				split.tabs.push(active);
			}

			let active = split.active_tab_id;
			let active_valid = active
				.map(|id| id.0 != 0 && valid_tabs.contains(&id) && window_tabs.contains(&id))
				.unwrap_or(false);
			if !active_valid {
				split.active_tab_id = split
					.tabs
					.iter()
					.find(|id| id.0 != 0)
					.copied()
					.or(default_tab);
			}

			if let Some(active) = split.active_tab_id
				&& active.0 != 0
				&& !split.tabs.contains(&active)
			{
				split.tabs.push(active);
			}
		}

		if layout.splits.is_empty() {
			if let Some(first_tab) = window_tabs.first().copied() {
				layout.splits.push(SplitPanel::new("split-1".to_string(), first_tab));
			} else {
				layout.splits.push(SplitPanel::new("split-1".to_string(), TabId(0)));
			}
		}

		while layout.splits.len() < layout.desired_split_count {
			let id = format!("split-{}", layout.splits.len() + 1);
			layout.splits.push(SplitPanel::new(id, TabId(0)));
		}

		if layout.active_split_index >= layout.splits.len() {
			layout.active_split_index = 0;
		}

		ensure_split_proportions(&mut layout.split_proportions, layout.splits.len());
	}

	fn render_split_panel(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let t = theme::theme();
		let layout = self.active_layout().unwrap();
		let split = layout.splits.get(index).unwrap();
		let split_tab_id = split.active_tab_id.unwrap_or(TabId(0));
		let window_active_tab_id = self
			.app_state
			.read(cx)
			.windows
			.get(&self.bound_window)
			.and_then(|w| w.active_tab)
			.unwrap_or(TabId(0));

		let mut tab_id = split_tab_id;
		if index == layout.active_split_index && window_active_tab_id.0 != 0 {
			tab_id = window_active_tab_id;
		}

		if index == layout.active_split_index && tab_id.0 != 0 {
			if let Some(layout) = self.active_layout_mut()
				&& let Some(split) = layout.splits.get_mut(index)
				&& split.active_tab_id != Some(tab_id)
			{
				split.active_tab_id = Some(tab_id);
				if !split.tabs.contains(&tab_id) {
					split.tabs.push(tab_id);
				}
			}
		}

		let (title, item_count, has_tab, badges) = {
			let app = self.app_state.read(cx);
			let tab = app.tabs.get(&tab_id);
			let title = tab.map(|tab| tab.title.clone()).unwrap_or_else(|| "No chat".to_string());
			let count = tab.map(|tab| tab.log.items.len()).unwrap_or(0);
			let badges = tab
				.map(|_| {
					app.rooms_for_tab(tab_id)
						.first()
						.map(|room| collect_badges_for_room(app, room))
						.unwrap_or_default()
				})
				.unwrap_or_default();
			(title, count, tab.is_some(), badges)
		};

		let (active_split_index, split_count, split_width, split_height) = {
			let layout = self.active_layout().unwrap();
			let width = split_content_width(index, &layout.split_proportions, self.split_bounds, layout.splits.len());
			let height = self.split_bounds.map(|b| b.size.height).unwrap_or(px(600.0));
			(layout.active_split_index, layout.splits.len(), width, height)
		};

		let view = cx.entity();
		let app_state = self.app_state.clone();
		let menu_state = self.message_menu.clone();
		let emote_image_cache = self.emote_image_cache.clone();
		let header_height = 36.0;
		let input_height = if has_tab { 44.0 } else { 0.0 };
		let split_height = px((f32::from(split_height) - header_height - input_height).max(0.0));
		let messages = render_split_messages(
			view,
			app_state,
			index,
			has_tab,
			item_count,
			split_width,
			split_height,
			tab_id,
			emote_image_cache,
			menu_state,
			&t,
			window,
			cx,
			move |this, window, cx| {
				if let Some(l) = this.active_layout_mut() {
					l.active_split_index = index;
				}
				this.add_tab(window, cx);
			},
			move |this, info, _window, cx| {
				this.message_menu = Some(info);
				cx.notify();
			},
			move |this, action, info, window, cx| {
				this.handle_message_action(action, info, window, cx);
			},
			move |this, _window, cx| {
				this.message_menu = None;
				cx.notify();
			},
		);

		let popout_index = index;
		let close_index = index;
		let can_close_split = split_count > 1;
		let input = if has_tab {
			let input_state = self.ensure_split_input(index, window, cx);
			Some(render_chat_input(input_state, index, &t))
		} else {
			None
		};

		let header = render_split_header(
			title,
			index == active_split_index,
			badges,
			self.emote_image_cache.clone(),
			can_close_split,
			cx,
			move |this, window, cx| {
				if let Some(l) = this.active_layout_mut() {
					l.active_split_index = index;
				}
				let win = this.bound_window;
				this.app_state.update(cx, |state, _cx| {
					state.set_active_tab(win, tab_id);
				});
				this.persist_layout(window, cx);
				cx.notify();
			},
			move |this, window, cx| {
				this.popout_split(popout_index, window, cx);
			},
			move |this, window, cx| {
				this.remove_split_at(close_index, window, cx);
			},
			move |this, window, cx| {
				this.change_split_stream(index, window, cx);
			},
		);

		let mut panel = div()
			.id(("split", index as u64))
			.flex()
			.flex_col()
			.h_full()
			.w(split_width)
			.flex_none()
			.min_w(px(220.0))
			.min_h(px(0.0))
			.bg(t.app_bg)
			.on_mouse_down(
				MouseButton::Left,
				cx.listener(move |this, _, window, cx| {
					let win = this.bound_window;
					if let Some(l) = this.active_layout_mut() {
						l.active_split_index = index;
					}
					this.app_state.update(cx, |state, _| {
						state.set_active_tab(win, tab_id);
					});
					this.persist_layout(window, cx);
					cx.notify();
				}),
			)
			.child(header)
			.child(messages);

		if let Some(input) = input {
			panel = panel.child(input);
		}

		panel
	}

	fn ensure_split_input(&mut self, split_index: usize, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
		if let Some(layout) = self.active_layout_mut()
			&& let Some(split) = layout.splits.get_mut(split_index)
		{
			if let Some(existing) = split.input.clone() {
				return existing;
			}

			let input = cx.new(|cx| InputState::new(window, cx).clean_on_escape().placeholder("Send message..."));
			split.input = Some(input.clone());

			let ent = input.clone();
			cx.subscribe_in(&ent, window, move |this, state: &Entity<InputState>, event, window, cx| {
				match event {
					InputEvent::Change => {
						if let Some(l) = this.active_layout_mut()
							&& let Some(split) = l.splits.get_mut(split_index)
						{
							split.draft = state.read(cx).value().to_string();
						}
					}
					InputEvent::PressEnter { .. } => {
						let text = state.read(cx).value().to_string();
						let tab_id = this
							.active_layout()
							.and_then(|l| l.splits.get(split_index))
							.and_then(|s| s.active_tab_id);
						if let Some(tab_id) = tab_id {
							this.send_message_to_tab(tab_id, text, cx);
						}

						state.update(cx, |s, cx| {
							s.set_value("", window, cx);
						});
					}
					_ => {}
				}
				// Use win_id if needed for persistence instead of captured window
			})
			.detach();

			return input;
		}

		cx.new(|cx| InputState::new(window, cx).clean_on_escape().placeholder("Send message..."))
	}

	fn active_tab_id(&self, cx: &mut Context<Self>) -> Option<TabId> {
		let app = self.app_state.read(cx);
		app.windows.get(&self.bound_window).and_then(|w| w.active_tab)
	}

	fn select_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
		let win = self.bound_window;
		self.app_state.update(cx, move |state, _cx| {
			state.set_active_tab(win, tab_id);
		});

		if let Some(layout) = self.active_layout_mut()
			&& let Some(split) = layout.splits.get_mut(layout.active_split_index)
		{
			split.active_tab_id = Some(tab_id);
		}

		self.persist_ui_state(cx);
		cx.notify();
	}

	fn toggle_pin_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
		let win = self.bound_window;
		self.app_state.update(cx, |state, _cx| {
			if let Some(tab) = state.tabs.get_mut(&tab_id) {
				tab.pinned = !tab.pinned;
			}

			let Some(window) = state.windows.get_mut(&win) else {
				return;
			};

			let mut pinned = Vec::new();
			let mut unpinned = Vec::new();

			for tab_id in window.tabs.drain(..) {
				if let Some(tab) = state.tabs.get(&tab_id) {
					if tab.pinned {
						pinned.push(tab_id);
					} else {
						unpinned.push(tab_id);
					}
				} else {
					unpinned.push(tab_id);
				}
			}

			window.tabs = pinned.into_iter().chain(unpinned).collect();
		});
		cx.notify();
	}

	fn send_message_to_tab(&mut self, tab_id: TabId, text: String, cx: &mut Context<Self>) {
		let text = text.trim().to_string();
		if text.is_empty() {
			return;
		}

		let Some(room) = self.room_for_tab(tab_id, cx) else {
			return;
		};

		let topic = RoomTopic::format(&room);
		if let Some(command) = Self::parse_command_input(&topic, &text) {
			self.send_command(command, cx);
			return;
		}
		if let Some(net) = self.net_controller.clone() {
			let app_state = self.app_state.clone();
			let text_for_cmd = text.clone();
			cx.spawn(async move |_, cx| {
				let cmd = pb::Command {
					command: Some(pb::command::Command::SendChat(pb::SendChatCommand {
						topic,
						text: text_for_cmd,
						reply_to_server_message_id: String::new(),
						reply_to_platform_message_id: String::new(),
					})),
				};
				if let Err(err) = net.send_command(cmd).await {
					app_state.update(cx, |state, _cx| {
						state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
					});
				}
			})
			.detach();
		} else {
			self.app_state.update(cx, |state, _cx| {
				state.push_notification(crate::ui::app_state::UiNotificationKind::Error, "not connected");
			});
			return;
		}

		let msg = ChatMessageUi {
			time: SystemTime::now(),
			platform: room.platform,
			room: room.clone(),
			server_message_id: None,
			author_id: None,
			user_login: "you".to_string(),
			user_display: None,
			text,
			badge_ids: Vec::new(),
			platform_message_id: None,
		};

		self.app_state.update(cx, |state, _cx| {
			let _ = state.push_message(msg);
		});
		cx.notify();
	}

	fn parse_command_input(topic: &str, text: &str) -> Option<pb::Command> {
		let trimmed = text.trim();
		let mut parts = trimmed.split_whitespace();
		let cmd = parts.next()?;

		match cmd {
			"/delete" => {
				let message_id = parts.next()?;
				Some(pb::Command {
					command: Some(pb::command::Command::DeleteMessage(pb::DeleteMessageCommand {
						topic: topic.to_string(),
						server_message_id: String::new(),
						platform_message_id: message_id.to_string(),
					})),
				})
			}
			"/timeout" => {
				let user_id = parts.next()?;
				let duration = parts.next()?;
				let duration_seconds = duration.parse::<u32>().ok()?;
				let reason = parts.collect::<Vec<_>>().join(" ");
				Some(pb::Command {
					command: Some(pb::command::Command::TimeoutUser(pb::TimeoutUserCommand {
						topic: topic.to_string(),
						user_id: user_id.to_string(),
						duration_seconds,
						reason,
					})),
				})
			}
			"/ban" => {
				let user_id = parts.next()?;
				let reason = parts.collect::<Vec<_>>().join(" ");
				Some(pb::Command {
					command: Some(pb::command::Command::BanUser(pb::BanUserCommand {
						topic: topic.to_string(),
						user_id: user_id.to_string(),
						reason,
					})),
				})
			}
			_ => None,
		}
	}

	fn handle_message_action(
		&mut self,
		action: MessageMenuAction,
		info: MessageContextInfo,
		window: &mut Window,
		cx: &mut Context<Self>,
	) {
		match action {
			MessageMenuAction::Reply => {
				let Some(platform_id) = info.platform_message_id.clone() else {
					self.app_state.update(cx, |state, _cx| {
						state.push_notification(
							crate::ui::app_state::UiNotificationKind::Error,
							"Message id not available for reply",
						);
					});
					return;
				};
				self.prefill_split_input(info.tab_id, format!("/reply {} ", platform_id), window, cx);
			}
			MessageMenuAction::Delete => {
				let Some(platform_id) = info.platform_message_id.clone() else {
					self.app_state.update(cx, |state, _cx| {
						state.push_notification(
							crate::ui::app_state::UiNotificationKind::Error,
							"Message id not available for delete",
						);
					});
					return;
				};
				self.prefill_split_input(info.tab_id, format!("/delete {}", platform_id), window, cx);
			}
			MessageMenuAction::Timeout => {
				let Some(author_id) = info.author_id.clone() else {
					self.app_state.update(cx, |state, _cx| {
						state.push_notification(
							crate::ui::app_state::UiNotificationKind::Error,
							"User id not available for timeout",
						);
					});
					return;
				};
				self.prefill_split_input(info.tab_id, format!("/timeout {} 600 ", author_id), window, cx);
			}
			MessageMenuAction::Ban => {
				let Some(author_id) = info.author_id.clone() else {
					self.app_state.update(cx, |state, _cx| {
						state.push_notification(
							crate::ui::app_state::UiNotificationKind::Error,
							"User id not available for ban",
						);
					});
					return;
				};
				self.prefill_split_input(info.tab_id, format!("/ban {} ", author_id), window, cx);
			}
		}

		self.message_menu = None;
		cx.notify();
	}

	fn prefill_split_input(&mut self, tab_id: TabId, text: String, window: &mut Window, cx: &mut Context<Self>) {
		let index = {
			let layout = match self.active_layout_mut() {
				Some(l) => l,
				None => return,
			};
			let Some(idx) = layout.splits.iter().position(|s| s.active_tab_id == Some(tab_id)) else {
				return;
			};
			layout.active_split_index = idx;
			idx
		};

		let input = self.ensure_split_input(index, window, cx);
		let text_clone = text.clone();
		input.update(cx, |state, cx| {
			state.set_value(text_clone, window, cx);
		});

		if let Some(layout) = self.active_layout_mut()
			&& let Some(split) = layout.splits.get_mut(index)
		{
			split.draft = text;
		}
		cx.notify();
	}

	fn send_command(&self, command: pb::Command, cx: &mut Context<Self>) {
		if let Some(net) = self.net_controller.clone() {
			let app_state = self.app_state.clone();
			cx.spawn(async move |_, cx| {
				if let Err(err) = net.send_command(command).await {
					app_state.update(cx, |state, _cx| {
						state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
					});
				}
			})
			.detach();
		} else {
			self.app_state.update(cx, |state, _cx| {
				state.push_notification(crate::ui::app_state::UiNotificationKind::Error, "not connected");
			});
		}
	}

	fn room_for_tab(&self, tab_id: TabId, cx: &mut Context<Self>) -> Option<RoomKey> {
		let app = self.app_state.read(cx);
		let tab = app.tabs.get(&tab_id)?;

		match &tab.target {
			TabTarget::Room(room) => Some(room.clone()),
			TabTarget::Group(group_id) => app.groups.get(group_id).and_then(|g| g.rooms.first().cloned()),
		}
	}

	fn add_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		let mut created_tab: Option<TabId> = None;

		let tab_id = self.active_tab_id(cx).unwrap_or_else(|| {
			let win = self.bound_window;
			let new_tab = self.app_state.update(cx, |state, _cx| {
				let room_id = RoomId::new("split".to_string()).unwrap_or_else(|_| RoomId::new("chat".to_string()).unwrap());
				let room_key = RoomKey::new(state.default_platform, room_id);
				let tab_id = state.create_tab_for_room("split", room_key);
				state.add_tab_to_window(win, tab_id);
				tab_id
			});
			created_tab = Some(new_tab);
			new_tab
		});

		if let Some(layout) = self.active_layout_mut() {
			let id = format!("split-{}", layout.splits.len() + 1);
			layout.splits.push(SplitPanel::new(id, tab_id));
			layout.desired_split_count = layout.splits.len();
			layout.use_fixed_split_widths = false;
			layout.split_proportions = rebalance_split_proportions(layout.splits.len());
		}

		if let Some(tab_id) = created_tab {
			self.subscribe_rooms_for_tab(tab_id, cx);
		}

		self.persist_layout(window, cx);
		self.open_join_dialog(None, window, cx);
		cx.notify();
	}

	fn remove_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		if let Some(layout) = self.active_layout() {
			let index = layout.active_split_index;
			self.remove_split_at(index, window, cx);
		}
	}

	fn remove_split_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
		let layout = match self.active_layout_mut() {
			Some(l) => l,
			None => return,
		};

		if layout.splits.len() <= 1 {
			if let Some(split) = layout.splits.get_mut(index) {
				split.active_tab_id = Some(TabId(0));
				self.persist_layout(window, cx);
				cx.notify();
			}
			return;
		}
		if index >= layout.splits.len() {
			return;
		}

		layout.splits.remove(index);
		if index < layout.split_proportions.len() {
			layout.split_proportions.remove(index);
		}

		if layout.active_split_index >= layout.splits.len() {
			layout.active_split_index = layout.splits.len().saturating_sub(1);
		} else if index <= layout.active_split_index && layout.active_split_index > 0 {
			layout.active_split_index = layout.active_split_index.saturating_sub(1);
		}

		layout.desired_split_count = layout.splits.len().max(1);
		layout.use_fixed_split_widths = false;
		layout.split_proportions = rebalance_split_proportions(layout.splits.len());
		self.persist_layout(window, cx);
		cx.notify();
	}

	fn begin_resize(&mut self, handle_index: usize, ev: &MouseDownEvent, cx: &mut Context<Self>) {
		let total_width = self.split_bounds.map(|b| b.size.width).unwrap_or(px(900.0));
		let props = {
			let layout = match self.active_layout_mut() {
				Some(l) => l,
				None => return,
			};
			layout.split_proportions.clone()
		};

		if begin_resize_drag(handle_index, ev, &props, &mut self.resize_drag, total_width) {
			cx.notify();
		}
	}

	fn on_resize_move(&mut self, ev: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
		let drag = match self.resize_drag.as_ref() {
			Some(d) => *d,
			None => return,
		};
		let layout = match self.active_layout_mut() {
			Some(l) => l,
			None => return,
		};
		let changed = update_resize_drag(ev, &mut layout.split_proportions, &Some(drag), default_min_split_width());
		if changed {
			cx.notify();
		}
	}

	fn on_resize_end(&mut self, _ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
		if end_resize_drag(&mut self.resize_drag) {
			self.persist_layout(window, cx);
		}
	}

	fn popout_active_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		if let Some(layout) = self.active_layout() {
			let active = layout.active_split_index;
			self.popout_split(active, window, cx);
		}
	}

	fn popout_split(&mut self, split_index: usize, window: &mut Window, cx: &mut Context<Self>) {
		let (tab_id, _split_id) = {
			let layout = self.active_layout().unwrap();
			let Some(split) = layout.splits.get(split_index) else {
				return;
			};
			(split.active_tab_id.unwrap_or(TabId(0)), split.id.clone())
		};

		if tab_id.0 == 0 {
			return;
		}

		let (title, room, group_id) = {
			let app = self.app_state.read(cx);
			let tab = app.tabs.get(&tab_id);
			let title = tab.map(|tab| tab.title.clone()).unwrap_or_else(|| "Chat".to_string());
			let (room, group_id) = match tab.map(|t| &t.target) {
				Some(TabTarget::Room(r)) => (Some(r.clone()), None),
				Some(TabTarget::Group(g)) => (None, Some(g.0)),
				_ => (None, None),
			};
			(title, room, group_id)
		};

		let app_ent = self.app_state.clone();
		let from_window = self.bound_window;
		let new_window_id = app_ent.update(cx, |state, _cx| {
			let new_win = state.create_window(format!("Popout — {}", title));
			state.move_tab(from_window, new_win, tab_id);
			new_win
		});

		let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
		let app_ent_for_window = app_ent.clone();
		let _ = cx.open_window(
			WindowOptions {
				window_bounds: Some(WindowBounds::Windowed(bounds)),
				kind: WindowKind::Floating,
				..Default::default()
			},
			move |window, cx| {
				let win_id = format!("window-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());
				let layout = settings::UiWindow {
					id: win_id.clone(),
					is_primary: false,
					x: f32::from(bounds.origin.x),
					y: f32::from(bounds.origin.y),
					width: f32::from(bounds.size.width),
					height: f32::from(bounds.size.height),
					active_layout_id: Some("layout-popout".to_string()),
					layouts: vec![settings::UiLayout {
						id: "layout-popout".to_string(),
						title: "Popout".to_string(),
						pinned: false,
						active_split_id: Some("split-popout".to_string()),
						splits: vec![settings::UiSplit {
							id: "split-popout".to_string(),
							width: 900.0,
							active_tab_id: Some(tab_id.0.to_string()),
							tabs: vec![settings::UiTab {
								id: tab_id.0.to_string(),
								title: title.clone(),
								room: room.clone(),
								group_id,
								pinned: false,
							}],
						}],
					}],
				};

				let view = cx.new(|cx| {
					MainPage::new(
						app_ent_for_window.clone(),
						new_window_id,
						None,
						RetainAllImageCache::new(cx),
						win_id,
						layout,
						false,
						cx,
					)
				});

				view.update(cx, |this, cx| this.persist_layout(window, cx));
				cx.new(|cx| Root::new(view, window, cx))
			},
		);

		if let Some(layout) = self.active_layout_mut() {
			layout.splits.retain(|s| s.active_tab_id != Some(tab_id));
			if layout.splits.is_empty() {
				layout.splits.push(SplitPanel::new("split-1".to_string(), TabId(0)));
			}
			layout.split_proportions = rebalance_split_proportions(layout.splits.len());
			layout.desired_split_count = layout.splits.len().max(1);
		}

		self.ensure_splits(cx);
		self.persist_layout(window, cx);
		window.refresh();
		cx.notify();
	}

	fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		self.open_join_dialog(None, window, cx);
	}

	fn close_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
		let rooms = self.app_state.read(cx).rooms_for_tab(tab_id);
		let closed = self.app_state.update(cx, |state, _cx| {
			state.close_tab(tab_id);
			true
		});

		if closed {
			self.unsubscribe_rooms_if_unused(rooms, cx);
			self.persist_ui_state(cx);
		}

		if let Some(layout) = self.active_layout_mut() {
			layout.splits.retain(|s| s.active_tab_id != Some(tab_id));
		}
		self.ensure_splits(cx);
		cx.notify();
	}

	fn change_split_stream(&mut self, split_index: usize, window: &mut Window, cx: &mut Context<Self>) {
		let view = cx.entity();
		let groups = self
			.app_state
			.read(cx)
			.groups
			.iter()
			.map(|(gid, g)| (gid.0, g.name.clone()))
			.collect();

		let current_name = if let Some(layout) = self.active_layout() {
			layout.splits.get(split_index).and_then(|s| {
				let app = self.app_state.read(cx);
				s.active_tab_id.and_then(|tid| app.tabs.get(&tid)).map(|t| t.title.clone())
			})
		} else {
			None
		};

		open_join_dialog(
			view,
			window,
			cx,
			groups,
			current_name,
			move |this, cx, submission| match submission {
				crate::ui::components::join_dialog::JoinSubmission::Single(raw) => {
					this.apply_join_raw_for_split(split_index, &raw, cx)
				}
				crate::ui::components::join_dialog::JoinSubmission::Group(raw) => {
					this.apply_join_group_for_split(split_index, &raw, cx)
				}
			},
		);
	}

	fn apply_join_raw_for_split(&mut self, split_index: usize, raw: &str, cx: &mut Context<Self>) -> bool {
		let old_tab_id = self
			.active_layout()
			.and_then(|l| l.splits.get(split_index))
			.and_then(|s| s.active_tab_id);

		if let Some(rest) = raw.strip_prefix("group:") {
			if let Ok(gid) = rest.parse::<u64>() {
				let group_id = crate::ui::app_state::GroupId(gid);
				let win = self.bound_window;
				let tab_id = self.app_state.update(cx, |state, _cx| {
					let title = state
						.group(group_id)
						.map(|g| g.name.clone())
						.unwrap_or_else(|| "group".to_string());
					let tab_id = state.create_tab_for_group(title, group_id);
					state.add_tab_to_window(win, tab_id);
					tab_id
				});
				self.update_split_tab(split_index, tab_id, cx);

				if let Some(old_id) = old_tab_id
					&& old_id != tab_id
					&& old_id.0 != 0
				{
					self.app_state.update(cx, |state, _cx| {
						state.close_tab(old_id);
					});
				}

				return true;
			}
			return false;
		}

		let default_platform = self.app_state.read(cx).default_platform;
		let room = if let Ok(room) = chatty_domain::RoomKey::parse(raw) {
			room
		} else {
			match chatty_domain::RoomId::new(raw.to_string()) {
				Ok(id) => chatty_domain::RoomKey::new(default_platform, id),
				Err(_) => return false,
			}
		};

		let win = self.bound_window;
		let tab_id = self.app_state.update(cx, |state, _cx| {
			let tab_id = state.create_tab_for_room(room.room_id.to_string(), room);
			state.add_tab_to_window(win, tab_id);
			tab_id
		});
		self.update_split_tab(split_index, tab_id, cx);

		if let Some(old_id) = old_tab_id
			&& old_id != tab_id
			&& old_id.0 != 0
		{
			self.app_state.update(cx, |state, _cx| {
				state.close_tab(old_id);
			});
		}

		true
	}

	fn apply_join_group_for_split(&mut self, split_index: usize, raw: &str, cx: &mut Context<Self>) -> bool {
		let mut rooms = Vec::new();
		let default_platform = self.app_state.read(cx).default_platform;

		for entry in raw.split([',', '\n']) {
			let item = entry.trim();
			if item.is_empty() {
				continue;
			}
			if let Ok(room) = chatty_domain::RoomKey::parse(item) {
				rooms.push(room);
				continue;
			}
			if let Ok(room_id) = chatty_domain::RoomId::new(item.to_string()) {
				rooms.push(chatty_domain::RoomKey::new(default_platform, room_id));
			}
		}

		if rooms.is_empty() {
			return false;
		}

		let old_tab_id = self
			.active_layout()
			.and_then(|l| l.splits.get(split_index))
			.and_then(|s| s.active_tab_id);

		let win = self.bound_window;
		let tab_id = self.app_state.update(cx, |state, _cx| {
			let gid = state.create_group("New Group", rooms);
			let tab_id = state.create_tab_for_group("New Group", gid);
			state.add_tab_to_window(win, tab_id);
			tab_id
		});
		self.update_split_tab(split_index, tab_id, cx);

		if let Some(old_id) = old_tab_id
			&& old_id != tab_id
			&& old_id.0 != 0
		{
			self.app_state.update(cx, |state, _cx| {
				state.close_tab(old_id);
			});
		}
		true
	}

	fn update_split_tab(&mut self, split_index: usize, tab_id: TabId, cx: &mut Context<Self>) {
		if let Some(layout) = self.active_layout_mut()
			&& let Some(split) = layout.splits.get_mut(split_index)
		{
			split.active_tab_id = Some(tab_id);
			if !split.tabs.contains(&tab_id) {
				split.tabs.push(tab_id);
			}
		}
		self.subscribe_rooms_for_tab(tab_id, cx);
		self.persist_ui_state(cx);
		cx.notify();
	}
}

impl gpui::Render for MainPage {
	fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let t = theme::theme();
		let dialog_layer = Root::render_dialog_layer(window, cx);

		div()
			.id("chatty-main")
			.size_full()
			.flex()
			.flex_col()
			.justify_start()
			.items_stretch()
			.bg(t.app_bg)
			.child(div().flex_none().w_full().child(self.render_topbar(window, cx)))
			.child(div().flex_none().w_full().child(self.render_tabs(window, cx)))
			.child(
				div()
					.flex()
					.flex_1()
					.min_h(px(0.0))
					.child(self.render_main_content(window, cx)),
			)
			.child(self.render_footer(window, cx))
			.children(dialog_layer)
	}
}

fn collect_badges_for_room(app: &AppState, room: &RoomKey) -> Vec<AssetRefUi> {
	let mut out = Vec::new();
	let mut seen = std::collections::HashSet::new();

	for key in &app.global_asset_cache_keys {
		if let Some(bundle) = app.asset_bundles.get(key) {
			for badge in &bundle.badges {
				if seen.insert(badge.id.clone()) {
					out.push(badge.clone());
				}
			}
		}
	}

	if let Some(keys) = app.room_asset_cache_keys.get(room) {
		for key in keys {
			if let Some(bundle) = app.asset_bundles.get(key) {
				for badge in &bundle.badges {
					if seen.insert(badge.id.clone()) {
						out.push(badge.clone());
					}
				}
			}
		}
	}

	out.sort_by(|a, b| cmp_badge_ids(&a.id, &b.id));
	out
}
