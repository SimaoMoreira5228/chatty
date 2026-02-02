use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiRootState {
	pub tabs: Vec<UiTab>,
	pub selected_tab_index: usize,
	#[serde(default)]
	pub windows: Vec<UiWindow>,
	#[serde(default)]
	pub geometry: Option<WindowGeometry>,
}

use iced::widget::pane_grid;

use crate::ui::components::tab::TabId;
use crate::ui::components::window::{WindowId, WindowModel};

impl UiRootState {
	pub fn from_app(app: &crate::app::Chatty) -> Self {
		fn find_focused_path(node: &pane_grid::Node, target: pane_grid::Pane, path: &mut Vec<bool>) -> bool {
			match node {
				pane_grid::Node::Pane(p) => *p == target,
				pane_grid::Node::Split { a, b, .. } => {
					path.push(false);
					if find_focused_path(a, target, path) {
						return true;
					}
					path.pop();
					path.push(true);
					if find_focused_path(b, target, path) {
						return true;
					}
					path.pop();
					false
				}
			}
		}

		fn build_ui_node(node: &pane_grid::Node, panes: &std::collections::HashMap<pane_grid::Pane, UiPane>) -> UiNode {
			match node {
				pane_grid::Node::Pane(p) => {
					let ps = panes.get(p).cloned().unwrap_or_default();
					UiNode::Leaf(ps)
				}
				pane_grid::Node::Split { axis, ratio, a, b, .. } => UiNode::Split {
					axis: match axis {
						pane_grid::Axis::Vertical => UiAxis::Vertical,
						pane_grid::Axis::Horizontal => UiAxis::Horizontal,
					},
					ratio: *ratio,
					first: Box::new(build_ui_node(a, panes)),
					second: Box::new(build_ui_node(b, panes)),
				},
			}
		}

		let mut tabs: Vec<UiTab> = Vec::new();
		for tid in &app.state.tab_order {
			if let Some(tab) = app.state.tabs.get(tid) {
				let mut pane_layouts: std::collections::HashMap<pane_grid::Pane, UiPane> = std::collections::HashMap::new();
				for (pane_id, pane_state) in tab.panes.iter() {
					let rooms = pane_state
						.tab_id
						.and_then(|tid| app.state.tabs.get(&tid).map(|t| t.target.0.clone()))
						.unwrap_or_default();
					pane_layouts.insert(
						*pane_id,
						UiPane {
							rooms,
							join_raw: pane_state.join_raw.clone(),
							composer: pane_state.composer.clone(),
						},
					);
				}

				let mut focused_path = Vec::new();
				let layout_tree = tab.panes.layout();
				if let Some(focused) = tab.focused_pane {
					find_focused_path(layout_tree, focused, &mut focused_path);
				}

				tabs.push(UiTab {
					title: tab.title.clone(),
					rooms: tab.target.0.clone(),
					pinned: tab.pinned,
					root: build_ui_node(layout_tree, &pane_layouts),
					focused_leaf_path: focused_path,
				});
			}
		}

		let selected_tab_index = if let Some(sid) = app.state.selected_tab_id {
			app.state.tab_order.iter().position(|id| id == &sid).unwrap_or(0)
		} else {
			0
		};

		let windows = {
			let mut windows = Vec::new();
			for model in app.state.popped_windows.values() {
				if let Some(tab_id) = model.tabs.first()
					&& let Some(tab) = app.state.tabs.get(tab_id)
				{
					let mut pane_layouts: std::collections::HashMap<pane_grid::Pane, UiPane> =
						std::collections::HashMap::new();
					for (pane_id, pane_state) in tab.panes.iter() {
						let rooms = pane_state
							.tab_id
							.and_then(|tid| app.state.tabs.get(&tid).map(|t| t.target.0.clone()))
							.unwrap_or_default();
						pane_layouts.insert(
							*pane_id,
							UiPane {
								rooms,
								join_raw: pane_state.join_raw.clone(),
								composer: pane_state.composer.clone(),
							},
						);
					}

					let mut focused_path = Vec::new();
					let layout_tree = tab.panes.layout();
					if let Some(focused) = tab.focused_pane {
						find_focused_path(layout_tree, focused, &mut focused_path);
					}

					windows.push(UiWindow {
						title: tab.title.clone(),
						tab: UiTab {
							title: tab.title.clone(),
							rooms: tab.target.0.clone(),
							pinned: tab.pinned,
							root: build_ui_node(layout_tree, &pane_layouts),
							focused_leaf_path: focused_path,
						},
						geometry: WindowGeometry {
							width: model.width,
							height: model.height,
							x: model.x,
							y: model.y,
						},
					});
				}
			}
			windows
		};

		Self {
			tabs,
			selected_tab_index,
			windows,
			geometry: Some(app.state.main_window_geometry.clone()),
		}
	}

	pub fn apply_to(&self, app: &mut crate::app::Chatty) {
		use crate::app::state::AppState;
		use crate::ui::components::chat_pane::ChatPane;

		app.state.tabs.clear();
		app.state.tab_order.clear();
		app.state.popped_windows.clear();
		app.state.pending_popped_tabs.clear();
		app.state.pending_restore_windows.clear();
		app.state.selected_tab_id = None;

		fn ensure_tab_for_rooms(state: &mut AppState, rooms: Vec<chatty_domain::RoomKey>) -> TabId {
			if rooms.is_empty() {
				return TabId(0);
			}

			let mut sorted_rooms = rooms.clone();
			sorted_rooms.sort_by(|a, b| {
				a.platform
					.as_str()
					.cmp(b.platform.as_str())
					.then_with(|| a.room_id.as_str().cmp(b.room_id.as_str()))
			});

			if let Some((tid, _)) = state.tabs.iter().find(|(_, t)| {
				let mut t_rooms = t.target.0.clone();
				t_rooms.sort_by(|a, b| {
					a.platform
						.as_str()
						.cmp(b.platform.as_str())
						.then_with(|| a.room_id.as_str().cmp(b.room_id.as_str()))
				});
				t_rooms == sorted_rooms
			}) {
				return *tid;
			}

			let title = rooms.iter().map(|r| r.room_id.as_str()).collect::<Vec<_>>().join(", ");
			state.create_tab_for_rooms(title, rooms)
		}

		fn build_tab_content(t: &UiTab, state: &mut AppState) -> TabId {
			let id = state.create_tab_for_rooms(t.title.clone(), t.rooms.clone());

			let mut pane_states = std::collections::HashMap::new();

			fn build_skeleton(
				node: &UiNode,
				panes: &mut pane_grid::State<ChatPane>,
				pane_id: pane_grid::Pane,
				leaf_data: &mut std::collections::HashMap<pane_grid::Pane, UiPane>,
			) {
				match node {
					UiNode::Leaf(lp) => {
						leaf_data.insert(pane_id, lp.clone());
					}
					UiNode::Split {
						axis,
						ratio,
						first,
						second,
					} => {
						if let Some((new_pane, split)) = panes.split(
							match axis {
								UiAxis::Vertical => pane_grid::Axis::Vertical,
								UiAxis::Horizontal => pane_grid::Axis::Horizontal,
							},
							pane_id,
							ChatPane::new(None),
						) {
							build_skeleton(first, panes, pane_id, leaf_data);
							build_skeleton(second, panes, new_pane, leaf_data);
							panes.resize(split, *ratio);
						}
					}
				}
			}

			let (mut panes, initial_pane) = pane_grid::State::new(ChatPane::new(Some(id)));

			build_skeleton(&t.root, &mut panes, initial_pane, &mut pane_states);

			for (pid, lp) in pane_states {
				if let Some(ps) = panes.get_mut(pid) {
					let pane_tab = if lp.rooms.is_empty() {
						id
					} else {
						ensure_tab_for_rooms(state, lp.rooms.clone())
					};
					ps.tab_id = Some(pane_tab);
					ps.join_raw = lp.join_raw;
					ps.composer = lp.composer;
				}
			}

			if let Some(tab) = state.tabs.get_mut(&id) {
				tab.panes = panes;
				tab.pinned = t.pinned;

				fn find_pane_by_path(node: &pane_grid::Node, path: &[bool]) -> pane_grid::Pane {
					let mut cur = node;
					for &side in path {
						match cur {
							pane_grid::Node::Split { a, b, .. } => {
								cur = if side { b } else { a };
							}
							pane_grid::Node::Pane(p) => return *p,
						}
					}
					match cur {
						pane_grid::Node::Pane(p) => *p,
						pane_grid::Node::Split { a, .. } => find_pane_by_path(a, &[]),
					}
				}

				tab.focused_pane = Some(find_pane_by_path(tab.panes.layout(), &t.focused_leaf_path));
			}

			id
		}

		let mut keep_ids: Vec<TabId> = Vec::new();
		for (i, t) in self.tabs.iter().enumerate() {
			let id = build_tab_content(t, &mut app.state);
			app.state.tab_order.push(id);
			keep_ids.push(id);
			if i == self.selected_tab_index {
				app.state.selected_tab_id = Some(id);
			}
		}

		for w in &self.windows {
			let id = build_tab_content(&w.tab, &mut app.state);
			if let Some(pos) = app.state.tab_order.iter().position(|x| *x == id) {
				app.state.tab_order.remove(pos);
			}

			let win_model = WindowModel {
				id: WindowId(0),
				title: w.title.clone(),
				tabs: vec![id],
				active_tab: Some(id),
				width: w.geometry.width,
				height: w.geometry.height,
				x: w.geometry.x,
				y: w.geometry.y,
			};

			app.state.pending_restore_windows.push(win_model);
		}

		{
			let keep: std::collections::HashSet<TabId> = keep_ids.into_iter().collect();
			app.state.tab_order.retain(|id| keep.contains(id));
			let mut seen = std::collections::HashSet::new();
			app.state.tab_order.retain(|id| seen.insert(*id));
		}

		if let Some(geo) = &self.geometry {
			app.state.main_window_geometry = geo.clone();
		}

		if app.state.selected_tab_id.is_none() && !app.state.tab_order.is_empty() {
			app.state.selected_tab_id = Some(app.state.tab_order[0]);
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiWindow {
	pub tab: UiTab,
	pub title: String,
	pub geometry: WindowGeometry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowGeometry {
	pub width: u32,
	pub height: u32,
	pub x: i32,
	pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTab {
	pub title: String,
	pub rooms: Vec<chatty_domain::RoomKey>,
	pub pinned: bool,
	pub root: UiNode,
	pub focused_leaf_path: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiNode {
	Leaf(UiPane),
	Split {
		axis: UiAxis,
		ratio: f32,
		first: Box<UiNode>,
		second: Box<UiNode>,
	},
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum UiAxis {
	Horizontal,
	Vertical,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiPane {
	#[serde(default)]
	pub rooms: Vec<chatty_domain::RoomKey>,
	#[serde(default)]
	pub join_raw: String,
	#[serde(default)]
	pub composer: String,
}

pub fn load_ui_layout() -> Option<UiRootState> {
	let path = dirs::config_dir()?.join("chatty/ui_layout.json");
	if !path.exists() {
		return None;
	}
	let s = std::fs::read_to_string(path).ok()?;
	serde_json::from_str(&s).ok()
}

pub fn save_ui_layout(state: &UiRootState) {
	if let Some(path) = dirs::config_dir().map(|p| p.join("chatty/ui_layout.json")) {
		if let Some(parent) = path.parent() {
			let _ = std::fs::create_dir_all(parent);
		}
		if let Ok(s) = serde_json::to_string_pretty(state) {
			let _ = std::fs::write(path, s);
		}
	}
}
