#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};

use chatty_domain::RoomKey;
use iced::widget::pane_grid;
use smol_str::SmolStr;

use crate::app::features::chat::ChatPane;
use crate::app::view_models::{ChatMessageUi, SystemNoticeUi};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TabTarget(pub Vec<RoomKey>);

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ChatItem {
	ChatMessage(Box<ChatMessageUi>),
	SystemNotice(SystemNoticeUi),
}

#[derive(Debug, Clone)]
pub struct ChatLog {
	pub items: VecDeque<ChatItem>,
	pub max_items: usize,
}

impl ChatLog {
	pub fn new(max_items: usize) -> Self {
		Self {
			items: VecDeque::new(),
			max_items,
		}
	}

	pub fn push(&mut self, item: ChatItem) -> Vec<ChatItem> {
		self.items.push_back(item);
		let mut removed = Vec::new();
		while self.items.len() > self.max_items {
			if let Some(front) = self.items.pop_front() {
				removed.push(front);
			}
		}
		removed
	}
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TabModel {
	pub id: TabId,
	pub title: String,
	pub target: TabTarget,
	pub log: ChatLog,
	pub user_counts: HashMap<SmolStr, usize>,
	pub pinned: bool,
	pub panes: pane_grid::State<ChatPane>,
	pub focused_pane: Option<pane_grid::Pane>,
}
