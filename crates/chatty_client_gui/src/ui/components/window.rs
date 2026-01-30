use crate::ui::components::tab::TabId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WindowModel {
	pub id: WindowId,
	pub title: String,
	pub tabs: Vec<TabId>,
	pub active_tab: Option<TabId>,
	pub width: u32,
	pub height: u32,
	pub x: i32,
	pub y: i32,
}
