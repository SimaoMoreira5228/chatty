#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use chatty_domain::RoomKey;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum UiNode {
	Leaf(UiPane),
	Split {
		axis: UiAxis,
		ratio: f32,
		first: Box<UiNode>,
		second: Box<UiNode>,
	},
}

impl Default for UiNode {
	fn default() -> Self {
		UiNode::Leaf(UiPane::default())
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiPane {
	pub join_raw: String,
	pub composer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTab {
	pub title: String,
	pub room: Option<RoomKey>,
	pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiRootState {
	pub root: UiNode,
	pub focused_leaf_path: Vec<bool>,
	#[serde(default)]
	pub tabs: Vec<UiTab>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum UiAxis {
	Vertical,
	Horizontal,
}

fn ui_layout_path() -> Option<PathBuf> {
	dirs::home_dir().map(|home| home.join(".chatty").join("ui_layout.json"))
}

pub fn load_ui_layout() -> Option<UiRootState> {
	let p = ui_layout_path()?;
	let s = fs::read_to_string(&p).ok()?;
	serde_json::from_str::<UiRootState>(&s).ok()
}

pub fn save_ui_layout(layout: &UiRootState) {
	if let Some(p) = ui_layout_path() {
		if let Some(parent) = p.parent() {
			let _ = fs::create_dir_all(parent);
		}

		if let Ok(json_s) = serde_json::to_string_pretty(layout) {
			let _ = fs::write(p, json_s);
		}
	}
}

impl UiPane {
	pub fn default() -> Self {
		Self {
			join_raw: String::new(),
			composer: String::new(),
		}
	}
}

pub fn delete_ui_layout() {
	if let Some(p) = ui_layout_path() {
		let _ = fs::remove_file(p);
	}
}
