#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use crate::Quit;
use crate::ui::app_state::{AppState, ChatItem, SystemNoticeUi, WindowId};
use crate::ui::net::{NetController, ShutdownHandle, start_networking};
use crate::ui::settings;
use gpui::Entity;
use gpui_component::Root;

use gpui::RetainAllImageCache;
use gpui::prelude::*;
use gpui::{App, Bounds, Context, KeyBinding, SharedString, Window, WindowBounds, WindowKind, WindowOptions, div, px, size};

#[derive(Debug, Clone)]
pub(crate) struct ChannelEntry {
	pub id: u64,
	/// Display label.
	pub display_name: SharedString,
	/// Hover details.
	pub hover_lines: Vec<SharedString>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatLine {
	pub id: u64,
	pub ts_ms: u64,
	pub kind: ChatLineKind,
	pub nick: SharedString,
	pub text: SharedString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatLineKind {
	Message,
	System,
}

pub(crate) struct MainWindow {
	#[allow(dead_code)]
	pub net_controller: Option<NetController>,
	#[allow(dead_code)]
	pub net_shutdown: Option<ShutdownHandle>,
	#[allow(dead_code)]
	pub app_state: Entity<AppState>,
	pub main_page: Entity<crate::ui::pages::main_page::MainPage>,
}

pub fn open_all_windows(cx: &mut App) -> gpui::Result<()> {
	let root = settings::load_ui_layout().unwrap_or_default();
	let app_state = cx.new(|_cx| AppState::new());

	let mut windows_to_open = root.windows;
	if windows_to_open.is_empty() {
		windows_to_open.push(settings::UiWindow {
			id: "window-primary".to_string(),
			is_primary: true,
			x: 100.0,
			y: 100.0,
			width: 1200.0,
			height: 800.0,
			active_layout_id: None,
			layouts: Vec::new(),
		});
	}

	for win_state in windows_to_open {
		let bounds = Bounds {
			origin: gpui::Point {
				x: px(win_state.x),
				y: px(win_state.y),
			},
			size: size(px(win_state.width), px(win_state.height)),
		};

		let app_state = app_state.clone();
		let _window = cx.open_window(
			WindowOptions {
				window_bounds: Some(WindowBounds::Windowed(bounds)),
				kind: if win_state.is_primary {
					WindowKind::Normal
				} else {
					WindowKind::Floating
				},
				..Default::default()
			},
			|window, cx| {
				let view = cx.new(|cx| MainWindow::new(window, app_state, win_state, cx));
				cx.new(|cx| Root::new(view, window, cx))
			},
		)?;
	}

	cx.activate(true);
	Ok(())
}

impl MainWindow {
	pub fn new(
		_window: &mut Window,
		app_state: Entity<AppState>,
		win_state: settings::UiWindow,
		cx: &mut Context<Self>,
	) -> Self {
		cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

		let main_window_id: WindowId = app_state.update(cx, |state, _cx| state.create_window(&win_state.id));

		app_state.update(cx, |state, _cx| {
			if let Some(tab_id) = state.windows.get(&main_window_id).and_then(|w| w.active_tab) {
				if let Some(tab) = state.tabs.get_mut(&tab_id) {
					tab.log.push(ChatItem::SystemNotice(SystemNoticeUi {
						time: SystemTime::now(),
						text: "Chatty UI ready.".to_string(),
					}));
				}
			}
		});

		let (net_controller, mut ui_rx, net_shutdown) = start_networking();
		{
			let app_state_clone = app_state.clone();
			let net_clone = net_controller.clone();
			cx.spawn(async move |_, cx| {
				let rooms = app_state_clone.update(cx, |state, _cx| {
					state
						.windows
						.get(&main_window_id)
						.map(|w| {
							w.tabs
								.iter()
								.flat_map(|tab_id| state.rooms_for_tab(*tab_id))
								.collect::<Vec<_>>()
						})
						.unwrap_or_default()
				});
				for room in rooms {
					let _ = net_clone.subscribe_room_key(room).await;
				}
			})
			.detach();
		}

		let main_page = cx.new(|cx| {
			crate::ui::pages::main_page::MainPage::new(
				app_state.clone(),
				main_window_id,
				Some(net_controller.clone()),
				RetainAllImageCache::new(cx),
				win_state.id.clone(),
				win_state.clone(),
				win_state.is_primary,
				cx,
			)
		});

		{
			let net_clone = net_controller.clone();
			let app_state_clone = app_state.clone();
			let settings_snapshot = settings::get_cloned();
			cx.spawn(async move |_, cx| {
				let cfg = match settings::build_client_config(&settings_snapshot) {
					Ok(cfg) => cfg,
					Err(err) => {
						app_state_clone.update(cx, |state, _cx| {
							state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
						});
						chatty_client_core::ClientConfigV1::default()
					}
				};
				let _ = net_clone.connect(cfg).await;
			})
			.detach();
		}

		cx.spawn(async move |this, cx| {
			while let Some(ev) = ui_rx.recv().await {
				let _ = this.update(cx, |this, cx| {
					let app_state = this.app_state.clone();
					let window_id = main_window_id;
					app_state.update(cx, |state, _cx| {
						let commands =
							crate::ui::reducer::reduce(state, window_id, crate::ui::reducer::UiAction::NetEvent(ev));
						crate::ui::reducer::apply_commands(state, window_id, commands);
					});
				});
			}
		})
		.detach();

		Self {
			net_controller: Some(net_controller),
			net_shutdown: Some(net_shutdown),
			app_state,
			main_page,
		}
	}

	pub(crate) fn now_ms() -> u64 {
		SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
	}

	pub(crate) fn system_line(id: u64, text: impl Into<SharedString>) -> ChatLine {
		ChatLine {
			id,
			ts_ms: Self::now_ms(),
			kind: ChatLineKind::System,
			nick: "system".into(),
			text: text.into(),
		}
	}

	fn message_line(id: u64, nick: impl Into<SharedString>, text: impl Into<SharedString>) -> ChatLine {
		ChatLine {
			id,
			ts_ms: Self::now_ms(),
			kind: ChatLineKind::Message,
			nick: nick.into(),
			text: text.into(),
		}
	}

	pub(crate) fn fmt_mm_ss(ts_ms: u64) -> SharedString {
		let s = (ts_ms / 1000) % 60;
		let m = (ts_ms / 1000 / 60) % 60;
		SharedString::from(format!("{m:02}:{s:02}"))
	}
}

impl Render for MainWindow {
	fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let main_page = self.main_page.clone();
		let content = main_page.update(cx, |main_page: &mut crate::ui::pages::main_page::MainPage, cx| {
			main_page.render(window, cx).into_any_element()
		});
		div().id("main-window").size_full().child(content)
	}
}
