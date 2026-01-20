#![forbid(unsafe_code)]

use lru::LruCache;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::mpsc;

#[cfg(not(test))]
use tokio::sync::Semaphore;

use chatty_client_core::ClientConfigV1;
use chatty_client_ui::app_state::{AppState, JoinRequest, TabId, TabTarget, UiNotificationKind};
use chatty_client_ui::net::{self, NetController};
use chatty_client_ui::settings::{ShortcutKey, SplitLayoutKind, ThemeKind};
use chatty_domain::{Platform, RoomKey};
use iced::Task;
use iced::keyboard;
use iced::widget::image::Handle as ImageHandle;
use iced::widget::pane_grid;
use tokio::sync::Mutex;

use crate::app::net::{UiEventReceiver, recv_next};
use rust_i18n::t;

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum Message {
	Navigate(Page),
	SettingsCategorySelected(SettingsCategory),
	UsersFilterChanged(String),
	PlatformSelected(PlatformChoice),
	MaxLogItemsChanged(String),
	SplitLayoutSelected(SplitLayoutChoice),
	DragModifierSelected(ShortcutKeyChoice),
	CloseKeyChanged(String),
	NewKeyChanged(String),
	ReconnectKeyChanged(String),
	VimNavToggled(bool),
	VimLeftKeyChanged(String),
	VimDownKeyChanged(String),
	VimUpKeyChanged(String),
	VimRightKeyChanged(String),
	CursorMoved(f32, f32),
	WindowResized(f32, f32),

	CharPressed(char, keyboard::Modifiers),
	NamedKeyPressed(iced::keyboard::key::Named),
	OpenPlatformLogin(chatty_domain::Platform),

	ImportFromFilePressed,
	LayoutImportFileParsed(Result<crate::ui::layout::UiRootState, String>),
	ConfirmImport,
	CancelImport,
	ConfirmExport,
	CancelExport,
	ConfirmReset,
	CancelReset,
	CancelError,
	ChooseExportPathPressed,
	LayoutExportPathChosen(Option<std::path::PathBuf>),

	ExportLayoutPressed,
	ImportLayoutPressed,
	LayoutImportClipboard(Option<String>),
	ResetLayoutPressed,
	ModalDismissed,
	ServerEndpointChanged(String),
	ServerAuthTokenChanged(String),
	ConnectPressed,
	DisconnectPressed,
	ConnectFinished(Result<(), String>),

	ThemeSelected(ThemeChoice),

	PaneJoinChanged(pane_grid::Pane, String),
	PaneJoinPressed(pane_grid::Pane),
	PaneSubscribed(pane_grid::Pane, Result<(), String>),
	TabUnsubscribed(chatty_domain::RoomKey, Result<(), String>),

	PaneComposerChanged(pane_grid::Pane, String),
	PaneSendPressed(pane_grid::Pane),
	Sent(Result<(), String>),

	MessageActionButtonPressed(chatty_domain::RoomKey, Option<String>, Option<String>, Option<String>),
	ReplyToMessage(chatty_domain::RoomKey, Option<String>, Option<String>),
	DeleteMessage(chatty_domain::RoomKey, Option<String>, Option<String>),
	TimeoutUser(chatty_domain::RoomKey, String),
	BanUser(chatty_domain::RoomKey, String),

	PasteTwitchBlob,
	PasteKickBlob,
	ClipboardRead(ClipboardTarget, Option<String>),
	IdentityUse(String),
	IdentityToggle(String),
	IdentityRemove(String),
	ClearIdentity,

	PaneClicked(pane_grid::Pane),
	PaneResized(pane_grid::ResizeEvent),
	PaneDragged(pane_grid::DragEvent),
	SplitSpiral,
	SplitMasonry,
	SplitPressed,
	CloseFocused,
	DismissToast,
	ModifiersChanged(keyboard::Modifiers),
	LocaleSelected(String),
	AutoConnectToggled(bool),

	NetPolled(Option<chatty_client_ui::net::UiEvent>),
	NavigatePaneLeft,
	NavigatePaneDown,
	NavigatePaneUp,
	NavigatePaneRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardTarget {
	Twitch,
	Kick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeChoice(pub ThemeKind);

impl ThemeChoice {
	pub const ALL: [ThemeChoice; 10] = [
		ThemeChoice(ThemeKind::DarkAmethyst),
		ThemeChoice(ThemeKind::Dark),
		ThemeChoice(ThemeKind::Light),
		ThemeChoice(ThemeKind::Solarized),
		ThemeChoice(ThemeKind::HighContrast),
		ThemeChoice(ThemeKind::Ocean),
		ThemeChoice(ThemeKind::Dracula),
		ThemeChoice(ThemeKind::Gruvbox),
		ThemeChoice(ThemeKind::Nord),
		ThemeChoice(ThemeKind::Synthwave),
	];
}

impl std::fmt::Display for ThemeChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self.0 {
			ThemeKind::Dark => t!("theme.dark"),
			ThemeKind::Light => t!("theme.light"),
			ThemeKind::Solarized => t!("theme.solarized"),
			ThemeKind::HighContrast => t!("theme.high_contrast"),
			ThemeKind::Ocean => t!("theme.ocean"),
			ThemeKind::Dracula => t!("theme.dracula"),
			ThemeKind::Gruvbox => t!("theme.gruvbox"),
			ThemeKind::Nord => t!("theme.nord"),
			ThemeKind::Synthwave => t!("theme.synthwave"),
			ThemeKind::DarkAmethyst => t!("theme.dark_amethyst"),
		};
		write!(f, "{}", name)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitLayoutChoice(pub SplitLayoutKind);

impl SplitLayoutChoice {
	pub const ALL: [SplitLayoutChoice; 3] = [
		SplitLayoutChoice(SplitLayoutKind::Masonry),
		SplitLayoutChoice(SplitLayoutKind::Spiral),
		SplitLayoutChoice(SplitLayoutKind::Linear),
	];
}

impl std::fmt::Display for SplitLayoutChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self.0 {
			SplitLayoutKind::Spiral => t!("split.spiral"),
			SplitLayoutKind::Masonry => t!("split.masonry"),
			SplitLayoutKind::Linear => t!("split.linear"),
		};

		write!(f, "{}", name)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutKeyChoice(pub ShortcutKey);

impl ShortcutKeyChoice {
	pub const ALL: [ShortcutKeyChoice; 6] = [
		ShortcutKeyChoice(ShortcutKey::Alt),
		ShortcutKeyChoice(ShortcutKey::Control),
		ShortcutKeyChoice(ShortcutKey::Shift),
		ShortcutKeyChoice(ShortcutKey::Logo),
		ShortcutKeyChoice(ShortcutKey::Always),
		ShortcutKeyChoice(ShortcutKey::None),
	];
}

impl std::fmt::Display for ShortcutKeyChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self.0 {
			ShortcutKey::Alt => t!("shortcut.alt"),
			ShortcutKey::Control => t!("shortcut.control"),
			ShortcutKey::Shift => t!("shortcut.shift"),
			ShortcutKey::Logo => t!("shortcut.logo"),
			ShortcutKey::Always => t!("shortcut.always"),
			ShortcutKey::None => t!("shortcut.none"),
		};
		write!(f, "{}", name)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
	Main,
	Settings,
	Users,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
	General,
	Server,
	Accounts,
	Keybinds,
	Diagnostics,
}

impl SettingsCategory {
	pub const ALL: [SettingsCategory; 5] = [
		SettingsCategory::General,
		SettingsCategory::Server,
		SettingsCategory::Accounts,
		SettingsCategory::Keybinds,
		SettingsCategory::Diagnostics,
	];

	pub fn label_key(self) -> &'static str {
		match self {
			SettingsCategory::General => "settings.general",
			SettingsCategory::Server => "settings.server",
			SettingsCategory::Accounts => "settings.accounts",
			SettingsCategory::Keybinds => "settings.keybinds",
			SettingsCategory::Diagnostics => "settings.diagnostics",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformChoice(pub Platform);

impl PlatformChoice {
	pub const ALL: [PlatformChoice; 2] = [PlatformChoice(Platform::Twitch), PlatformChoice(Platform::Kick)];
}

impl std::fmt::Display for PlatformChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let label = match self.0 {
			Platform::Twitch => t!("platform.twitch"),
			Platform::Kick => t!("platform.kick"),
			_ => t!("platform.unknown"),
		};
		write!(f, "{}", label)
	}
}

#[derive(Debug, Clone)]
pub struct PaneState {
	pub(crate) tab_id: Option<TabId>,
	pub(crate) composer: String,
	pub(crate) join_raw: String,
	pub(crate) reply_to_server_message_id: String,
	pub(crate) reply_to_platform_message_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertTarget {
	Composer,
	Join,
}

pub struct Chatty {
	pub(crate) net: NetController,
	pub(crate) net_rx: Arc<Mutex<UiEventReceiver>>,
	pub(crate) _shutdown: Option<net::ShutdownHandle>,

	pub(crate) state: AppState,
	pub(crate) panes: pane_grid::State<PaneState>,
	pub(crate) focused_pane: pane_grid::Pane,
	pub(crate) page: Page,
	pub(crate) settings_category: SettingsCategory,

	pub(crate) server_endpoint_quic: String,
	pub(crate) server_auth_token: String,
	pub(crate) max_log_items_raw: String,
	pub(crate) users_filter_raw: String,

	pub(crate) spiral_dir: u8,
	pub(crate) masonry_flip: bool,
	pub(crate) modifiers: keyboard::Modifiers,

	pub(crate) window_size: Option<(f32, f32)>,
	pub(crate) toast: Option<String>,

	pub(crate) pending_import_root: Option<crate::ui::layout::UiRootState>,
	pub(crate) pending_export_root: Option<crate::ui::layout::UiRootState>,
	pub(crate) pending_export_path: Option<std::path::PathBuf>,
	pub(crate) pending_reset: bool,
	pub(crate) pending_error: Option<String>,
	pub(crate) pending_auto_connect_cfg: Option<ClientConfigV1>,

	pub(crate) image_cache: Arc<StdMutex<LruCache<String, ImageHandle>>>,
	pub(crate) image_loading: Arc<StdMutex<HashSet<String>>>,
	pub(crate) image_failed: Arc<StdMutex<HashSet<String>>>,
	pub(crate) pending_commands: Vec<PendingCommand>,
	pub(crate) image_fetch_sender: mpsc::Sender<String>,
	pub(crate) pending_message_action: Option<PendingMessageAction>,
	pub(crate) pending_deletion: Option<(chatty_domain::RoomKey, Option<String>, Option<String>)>,

	pub(crate) insert_mode: bool,
	pub(crate) insert_target: Option<InsertTarget>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PendingCommand {
	Delete {
		room: RoomKey,
		server_message_id: Option<String>,
		platform_message_id: Option<String>,
	},
	Timeout {
		room: RoomKey,
		user_id: String,
	},
	Ban {
		room: RoomKey,
		user_id: String,
	},
}

pub(crate) type PendingMessageAction = (chatty_domain::RoomKey, Option<String>, Option<String>, Option<String>);

pub(crate) fn first_char_lower(s: &str) -> char {
	s.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0')
}

impl Chatty {
	pub fn new() -> (Self, Task<Message>) {
		let (net, rx, shutdown) = net::start_networking();
		let net_rx = Arc::new(Mutex::new(rx));
		let state = AppState::new();
		let gs = state.gui_settings().clone();
		let (panes, root) = pane_grid::State::new(PaneState {
			tab_id: None,
			composer: String::new(),
			join_raw: String::new(),
			reply_to_server_message_id: String::new(),
			reply_to_platform_message_id: String::new(),
		});

		let initial_focused = root;

		let mut instance = Self {
			net,
			net_rx: net_rx.clone(),
			_shutdown: Some(shutdown),

			state,
			panes,
			focused_pane: initial_focused,
			page: Page::Main,
			settings_category: SettingsCategory::General,

			server_endpoint_quic: gs.server_endpoint_quic.clone(),
			server_auth_token: gs.server_auth_token.clone(),
			max_log_items_raw: gs.max_log_items.to_string(),
			users_filter_raw: String::new(),
			spiral_dir: 0,
			masonry_flip: false,
			modifiers: keyboard::Modifiers::default(),
			window_size: None,
			toast: None,
			pending_import_root: None,
			pending_export_root: None,
			pending_export_path: None,
			pending_reset: false,
			pending_error: None,
			insert_mode: false,
			insert_target: None,
			pending_auto_connect_cfg: None,
			image_cache: Arc::new(StdMutex::new(LruCache::new(NonZeroUsize::new(512).unwrap()))),
			image_loading: Arc::new(StdMutex::new(HashSet::new())),
			image_failed: Arc::new(StdMutex::new(HashSet::new())),
			pending_commands: Vec::new(),
			image_fetch_sender: {
				let (tx, _rx) = mpsc::channel::<String>(1);
				tx
			},
			pending_deletion: None,
			pending_message_action: None,
		};

		#[cfg(not(test))]
		{
			let image_cache_arc = Arc::clone(&instance.image_cache);
			let image_loading_arc = Arc::clone(&instance.image_loading);
			let image_failed_arc = Arc::clone(&instance.image_failed);
			let (img_tx, mut img_rx) = mpsc::channel::<String>(200);
			let sem: std::sync::Arc<tokio::sync::Semaphore> = Arc::new(Semaphore::new(6));

			instance.image_fetch_sender = img_tx.clone();

			tokio::spawn(async move {
				let client = reqwest::Client::new();
				while let Some(url) = img_rx.recv().await {
					let img_cache = Arc::clone(&image_cache_arc);
					let image_loading = Arc::clone(&image_loading_arc);
					let image_failed = Arc::clone(&image_failed_arc);
					let sem = Arc::clone(&sem);
					let client = client.clone();
					tokio::spawn(async move {
						let _permit = sem.acquire().await.expect("semaphore closed");

						{
							let guard = img_cache.lock().unwrap();
							if guard.contains(&url) {
								return;
							}
						}

						{
							let mut l = image_loading.lock().unwrap();
							l.insert(url.clone());
						}

						let mut succeeded = false;
						let mut attempt = 0u8;
						while attempt < 3 && !succeeded {
							attempt += 1;
							match tokio::time::timeout(std::time::Duration::from_secs(8), client.get(&url).send()).await {
								Ok(Ok(resp)) => {
									match tokio::time::timeout(std::time::Duration::from_secs(8), resp.bytes()).await {
										Ok(Ok(bytes)) => {
											let handle = iced::widget::image::Handle::from_bytes(bytes.to_vec());
											let mut g = img_cache.lock().unwrap();
											g.put(url.clone(), handle);
											succeeded = true;
										}
										_ => {
											if attempt < 3 {
												tokio::time::sleep(std::time::Duration::from_millis(300)).await;
											}
										}
									}
								}
								_ => {
									if attempt < 3 {
										tokio::time::sleep(std::time::Duration::from_millis(300)).await;
									}
								}
							}
						}

						{
							let mut l = image_loading.lock().unwrap();
							l.remove(&url);
						}

						if !succeeded {
							let mut f = image_failed.lock().unwrap();
							f.insert(url.clone());
						} else {
							let mut f = image_failed.lock().unwrap();
							f.remove(&url);
						}
					});
				}
			});
		}

		if let Some(layout) = crate::ui::layout::load_ui_layout() {
			instance.apply_ui_root(layout);
			instance.save_ui_layout();
		}

		let initial_task = if gs.auto_connect_on_startup {
			match chatty_client_ui::settings::build_client_config(&gs) {
				Ok(cfg) => {
					instance.pending_auto_connect_cfg = Some(cfg.clone());
					instance
						.state
						.set_connection_status(chatty_client_ui::app_state::ConnectionStatus::Connecting);
					let net_clone = instance.net.clone();
					let rx_clone = net_rx.clone();
					Task::perform(
						async move {
							let _ = net_clone.connect(cfg).await;
							recv_next(rx_clone).await
						},
						Message::NetPolled,
					)
				}
				Err(e) => {
					instance.state.push_notification(UiNotificationKind::Error, e);
					Task::perform(recv_next(net_rx.clone()), Message::NetPolled)
				}
			}
		} else {
			Task::perform(recv_next(net_rx), Message::NetPolled)
		};

		(instance, initial_task)
	}

	pub fn collect_orphaned_tab(&mut self) -> Option<chatty_domain::RoomKey> {
		let mut to_remove: Vec<(chatty_client_ui::app_state::TabId, chatty_domain::RoomKey)> = Vec::new();
		for (tid, tab) in &self.state.tabs {
			if let chatty_client_ui::app_state::TabTarget::Room(room) = &tab.target {
				let referenced = self.panes.iter().any(|(_, p)| p.tab_id == Some(*tid));
				if !referenced {
					to_remove.push((*tid, room.clone()));
				}
			}
		}

		if let Some((tid, room)) = to_remove.into_iter().next() {
			self.state.tabs.remove(&tid);
			Some(room)
		} else {
			None
		}
	}
	pub fn navigate_pane(&mut self, dx: i32, dy: i32) {
		let ids: Vec<pane_grid::Pane> = self.panes.iter().map(|(id, _)| *id).collect();
		if ids.is_empty() {
			return;
		}
		let pos = ids.iter().position(|p| *p == self.focused_pane).unwrap_or(0);
		let step = if dx < 0 || dy < 0 { -1isize } else { 1isize };
		let new_idx = ((pos as isize + step).rem_euclid(ids.len() as isize)) as usize;
		self.focused_pane = ids[new_idx];
	}

	pub fn set_focused_by_cursor(&mut self, x: f32, y: f32) {
		use iced::Size;
		if let Some((w, h)) = self.window_size {
			let bounds = Size::new(w, h);
			let regions = self.panes.layout().pane_regions(8.0, 50.0, bounds);
			for (pane, rect) in regions {
				if x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height {
					self.focused_pane = pane;
					break;
				}
			}
		}
	}

	pub fn capture_ui_root(&self) -> crate::ui::layout::UiRootState {
		use crate::ui::layout::UiTab;
		use iced::Size;

		let mut tabs: Vec<UiTab> = Vec::new();
		for (_id, tab) in self.state.tabs.iter() {
			tabs.push(UiTab {
				title: tab.title.clone(),
				room: match &tab.target {
					TabTarget::Room(r) => Some(r.clone()),
					_ => None,
				},
				pinned: tab.pinned,
			});
		}

		let (w, h) = self.window_size.unwrap_or((800.0, 600.0));
		let bounds = Size::new(w, h);
		let regions_map = self.panes.layout().pane_regions(8.0, 50.0, bounds);
		let regions_vec: Vec<(pane_grid::Pane, iced::Rectangle)> = regions_map.into_iter().collect();

		fn build_node(regions: &[(pane_grid::Pane, iced::Rectangle)], bounds: iced::Size) -> crate::ui::layout::UiNode {
			if regions.len() == 1 {
				let (_p, _r) = &regions[0];
				return crate::ui::layout::UiNode::Leaf(crate::ui::layout::UiPane::default());
			}

			let mut centers_x: Vec<f32> = regions.iter().map(|(_, r)| r.x + r.width / 2.0).collect();
			centers_x.sort_by(|a, b| a.partial_cmp(b).unwrap());
			let median_x = centers_x[centers_x.len() / 2];
			let mut left: Vec<(pane_grid::Pane, iced::Rectangle)> = Vec::new();
			let mut right: Vec<(pane_grid::Pane, iced::Rectangle)> = Vec::new();
			for it in regions.iter() {
				let c = it.1.x + it.1.width / 2.0;
				if c < median_x {
					left.push(*it);
				} else {
					right.push(*it);
				}
			}

			if !left.is_empty() && !right.is_empty() {
				let left_width = left.iter().map(|(_, r)| r.width).sum::<f32>();
				let right_width = right.iter().map(|(_, r)| r.width).sum::<f32>();
				let total_width = (left_width + right_width).max(1.0);
				let ratio = (left_width / total_width).clamp(0.05, 0.95);
				let first_bounds = iced::Size::new(left_width, bounds.height);
				let second_bounds = iced::Size::new(right_width, bounds.height);
				let first = build_node(&left, first_bounds);
				let second = build_node(&right, second_bounds);
				return crate::ui::layout::UiNode::Split {
					axis: crate::ui::layout::UiAxis::Vertical,
					ratio,
					first: Box::new(first),
					second: Box::new(second),
				};
			}

			let mut centers_y: Vec<f32> = regions.iter().map(|(_, r)| r.y + r.height / 2.0).collect();
			centers_y.sort_by(|a, b| a.partial_cmp(b).unwrap());
			let median_y = centers_y[centers_y.len() / 2];
			let mut top: Vec<(pane_grid::Pane, iced::Rectangle)> = Vec::new();
			let mut bottom: Vec<(pane_grid::Pane, iced::Rectangle)> = Vec::new();
			for it in regions.iter() {
				let c = it.1.y + it.1.height / 2.0;
				if c < median_y {
					top.push(*it);
				} else {
					bottom.push(*it);
				}
			}

			if !top.is_empty() && !bottom.is_empty() {
				let top_h = top.iter().map(|(_, r)| r.height).sum::<f32>();
				let bottom_h = bottom.iter().map(|(_, r)| r.height).sum::<f32>();
				let total_h = (top_h + bottom_h).max(1.0);
				let ratio = (top_h / total_h).clamp(0.05, 0.95);
				let first_bounds = iced::Size::new(bounds.width, top_h);
				let second_bounds = iced::Size::new(bounds.width, bottom_h);
				let first = build_node(&top, first_bounds);
				let second = build_node(&bottom, second_bounds);
				return crate::ui::layout::UiNode::Split {
					axis: crate::ui::layout::UiAxis::Horizontal,
					ratio,
					first: Box::new(first),
					second: Box::new(second),
				};
			}

			let mut items: Vec<(crate::ui::layout::UiNode, iced::Rectangle)> = regions
				.iter()
				.map(|(_, r)| (crate::ui::layout::UiNode::Leaf(crate::ui::layout::UiPane::default()), *r))
				.collect();

			while items.len() > 1 {
				let mut best = (0usize, 1usize, f32::MAX);
				for i in 0..items.len() {
					for j in (i + 1)..items.len() {
						let ri = items[i].1;
						let rj = items[j].1;
						let ci = (ri.x + ri.width / 2.0, ri.y + ri.height / 2.0);
						let cj = (rj.x + rj.width / 2.0, rj.y + rj.height / 2.0);
						let dx = ci.0 - cj.0;
						let dy = ci.1 - cj.1;
						let dist = dx * dx + dy * dy;
						if dist < best.2 {
							best = (i, j, dist);
						}
					}
				}

				let (i, j, _) = best;
				let (node_a, rect_a) = items.remove(j);
				let (node_b, rect_b) = items.remove(i);

				let ca = (rect_a.x + rect_a.width / 2.0, rect_a.y + rect_a.height / 2.0);
				let cb = (rect_b.x + rect_b.width / 2.0, rect_b.y + rect_b.height / 2.0);
				let axis = if (ca.0 - cb.0).abs() > (ca.1 - cb.1).abs() {
					crate::ui::layout::UiAxis::Vertical
				} else {
					crate::ui::layout::UiAxis::Horizontal
				};

				let ratio = match axis {
					crate::ui::layout::UiAxis::Vertical => (rect_a.width / (rect_a.width + rect_b.width)).clamp(0.05, 0.95),
					crate::ui::layout::UiAxis::Horizontal => {
						(rect_a.height / (rect_a.height + rect_b.height)).clamp(0.05, 0.95)
					}
				};

				let merged_rect = iced::Rectangle {
					x: rect_a.x.min(rect_b.x),
					y: rect_a.y.min(rect_b.y),
					width: (rect_a.x + rect_a.width).max(rect_b.x + rect_b.width) - rect_a.x.min(rect_b.x),
					height: (rect_a.y + rect_a.height).max(rect_b.y + rect_b.height) - rect_a.y.min(rect_b.y),
				};

				let merged_node = crate::ui::layout::UiNode::Split {
					axis,
					ratio,
					first: Box::new(node_a),
					second: Box::new(node_b),
				};

				items.push((merged_node, merged_rect));
			}

			items.into_iter().next().map(|(n, _)| n).unwrap_or_default()
		}

		let root_node = build_node(&regions_vec, bounds);

		let ids: Vec<pane_grid::Pane> = self.panes.iter().map(|(id, _)| *id).collect();
		let focused_idx = ids.iter().position(|p| *p == self.focused_pane).unwrap_or(0);

		fn index_to_path(node: &crate::ui::layout::UiNode, mut target: usize) -> Vec<bool> {
			fn count_leaves(node: &crate::ui::layout::UiNode) -> usize {
				match node {
					crate::ui::layout::UiNode::Leaf(_) => 1,
					crate::ui::layout::UiNode::Split { first, second, .. } => count_leaves(first) + count_leaves(second),
				}
			}
			let mut path = Vec::new();
			let mut cur = node;
			loop {
				match cur {
					crate::ui::layout::UiNode::Leaf(_) => break,
					crate::ui::layout::UiNode::Split { first, second, .. } => {
						let left_count = count_leaves(first);
						if target < left_count {
							path.push(false);
							cur = first;
						} else {
							target -= left_count;
							path.push(true);
							cur = second;
						}
					}
				}
			}
			path
		}

		let focused_path = index_to_path(&root_node, focused_idx);

		crate::ui::layout::UiRootState {
			root: root_node,
			focused_leaf_path: focused_path,
			tabs,
		}
	}

	pub fn apply_ui_root(&mut self, root: crate::ui::layout::UiRootState) {
		use crate::ui::layout::{UiAxis, UiNode};

		let (mut new_panes, new_root) = pane_grid::State::new(PaneState {
			tab_id: None,
			composer: String::new(),
			join_raw: String::new(),
			reply_to_server_message_id: String::new(),
			reply_to_platform_message_id: String::new(),
		});

		fn build_on_pane(node: &UiNode, panes: &mut pane_grid::State<PaneState>, pane_id: pane_grid::Pane) {
			match node {
				UiNode::Leaf(up) => {
					if let Some(p) = panes.get_mut(pane_id) {
						p.composer = up.composer.clone();
						p.join_raw = up.join_raw.clone();
					}
				}
				UiNode::Split {
					axis,
					ratio,
					first,
					second,
				} => {
					let new_state = PaneState {
						tab_id: None,
						composer: String::new(),
						join_raw: String::new(),
						reply_to_server_message_id: String::new(),
						reply_to_platform_message_id: String::new(),
					};
					if let Some((new_pane, split)) = panes.split(
						match axis {
							UiAxis::Vertical => pane_grid::Axis::Vertical,
							UiAxis::Horizontal => pane_grid::Axis::Horizontal,
						},
						pane_id,
						new_state,
					) {
						build_on_pane(first, panes, pane_id);
						build_on_pane(second, panes, new_pane);
						panes.resize(split, *ratio);
					}
				}
			}
		}

		build_on_pane(&root.root, &mut new_panes, new_root);

		self.panes = new_panes;

		fn index_of_path(node: &crate::ui::layout::UiNode, path: &[bool]) -> Option<usize> {
			fn count_leaves(node: &crate::ui::layout::UiNode) -> usize {
				match node {
					crate::ui::layout::UiNode::Leaf(_) => 1,
					crate::ui::layout::UiNode::Split { first, second, .. } => count_leaves(first) + count_leaves(second),
				}
			}
			let mut cur = node;
			let mut offset = 0usize;
			for &b in path.iter() {
				match cur {
					crate::ui::layout::UiNode::Leaf(_) => return None,
					crate::ui::layout::UiNode::Split { first, second, .. } => {
						if !b {
							cur = first;
						} else {
							offset += count_leaves(first);
							cur = second;
						}
					}
				}
			}

			Some(offset)
		}

		if let Some(fi) = index_of_path(&root.root, &root.focused_leaf_path) {
			let ids: Vec<pane_grid::Pane> = self.panes.iter().map(|(id, _)| *id).collect();
			if fi < ids.len() {
				self.focused_pane = ids[fi];
			}
		}

		for t in root.tabs {
			if let Some(room) = t.room {
				let tid = self.ensure_tab_for_room(&room);
				if let Some(tab) = self.state.tabs.get_mut(&tid) {
					tab.title = t.title.clone();
					tab.pinned = t.pinned;
				}
			}
		}
	}

	pub fn save_ui_layout(&self) {
		let root = self.capture_ui_root();
		crate::ui::layout::save_ui_layout(&root);
	}

	pub fn view(&self) -> iced::Element<'_, Message> {
		crate::ui::view(self)
	}

	pub fn pane_drag_enabled(&self) -> bool {
		let gs = self.state.gui_settings();
		match gs.keybinds.drag_modifier {
			ShortcutKey::Always => true,
			ShortcutKey::Alt => self.modifiers.alt(),
			ShortcutKey::Control => self.modifiers.control(),
			ShortcutKey::Shift => self.modifiers.shift(),
			ShortcutKey::Logo => self.modifiers.logo(),
			ShortcutKey::None => false,
		}
	}

	pub(crate) fn focused_tab_id(&self) -> Option<TabId> {
		self.panes.get(self.focused_pane).and_then(|p| p.tab_id)
	}

	pub(crate) fn pane_room(&self, pane: pane_grid::Pane) -> Option<RoomKey> {
		let tab_id = self.panes.get(pane).and_then(|p| p.tab_id)?;
		let tab = self.state.tabs.get(&tab_id)?;
		match &tab.target {
			TabTarget::Room(room) => Some(room.clone()),
			_ => None,
		}
	}

	pub(crate) fn ensure_tab_for_room(&mut self, room: &RoomKey) -> TabId {
		if let Some((tid, _)) = self
			.state
			.tabs
			.iter()
			.find(|(_, t)| matches!(&t.target, TabTarget::Room(rk) if rk == room))
		{
			return *tid;
		}

		let title = format!("{}:{}", room.platform.as_str(), room.room_id.as_str());
		self.state.create_tab_for_room(title, room.clone())
	}

	pub(crate) fn active_identity_label(&self) -> String {
		let gs = self.state.gui_settings();
		if let Some(id) = gs.active_identity.as_ref()
			&& let Some(identity) = gs.identities.iter().find(|i| &i.id == id)
		{
			return format!("{} ({:?})", identity.display_name, identity.platform);
		}
		"None".to_string()
	}

	#[allow(dead_code)]
	pub(crate) fn invalid_room_toast(&mut self) {
		self.toast = Some(t!("invalid_room").to_string());
		self.state
			.push_notification(UiNotificationKind::Warning, t!("invalid_room").to_string());
	}

	#[allow(dead_code)]
	pub(crate) fn apply_join_request(&mut self, pane: pane_grid::Pane) -> Option<(RoomKey, JoinRequest)> {
		let raw = self.panes.get(pane).map(|p| p.join_raw.clone()).unwrap_or_default();
		let req = JoinRequest { raw };
		let room = self.state.parse_join_room(&req)?;
		Some((room, req))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	fn roundtrip_for_layout<F>(mut make_layout: F)
	where
		F: FnMut(&mut Chatty),
	{
		let td = tempdir().expect("tempdir");

		let (mut inst, _task) = Chatty::new();
		inst.window_size = Some((800.0, 600.0));

		let room = chatty_domain::RoomKey::new(
			chatty_domain::Platform::Twitch,
			chatty_domain::RoomId::new("room1").expect("room id"),
		);
		let tid = inst.ensure_tab_for_room(&room);
		if let Some(tab) = inst.state.tabs.get_mut(&tid) {
			tab.title = "room1 title".to_string();
			tab.pinned = true;
		}

		for _ in 0..4 {
			make_layout(&mut inst);
		}

		let ids: Vec<pane_grid::Pane> = inst.panes.iter().map(|(id, _)| *id).collect();
		for id in ids {
			if let Some(p) = inst.panes.get_mut(id) {
				p.composer = "composer".to_string();
				p.join_raw = "join".to_string();
			}
		}

		let root = inst.capture_ui_root();

		let p = td.path().join(".chatty").join("ui_layout.json");
		if let Some(parent) = p.parent() {
			let _ = std::fs::create_dir_all(parent);
		}
		let json_s = serde_json::to_string_pretty(&root).expect("serialize");
		std::fs::write(&p, json_s).expect("write");

		let loaded_s = std::fs::read_to_string(&p).expect("read");
		let loaded = serde_json::from_str::<crate::ui::layout::UiRootState>(&loaded_s).expect("loaded");
		let a = serde_json::to_value(&root).unwrap();
		let b = serde_json::to_value(&loaded).unwrap();
		assert_eq!(a, b, "roundtrip must preserve layout");
	}

	#[test]
	fn layout_roundtrip_spiral() {
		roundtrip_for_layout(|i| i.split_spiral());
	}

	#[test]
	fn layout_roundtrip_masonry() {
		roundtrip_for_layout(|i| i.split_masonry());
	}

	#[test]
	fn layout_roundtrip_linear() {
		roundtrip_for_layout(|i| i.split_linear());
	}
}
