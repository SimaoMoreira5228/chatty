use std::time::Instant;

use chatty_domain::RoomKey;
use iced::keyboard;
use iced::widget::{pane_grid, scrollable, text_editor};

use crate::app::features::chat::ChatPaneMessage;
use crate::app::features::settings::SettingsMessage;
use crate::app::features::tabs::TabId;
use crate::app::features::users::UsersViewMessage;
use crate::app::types::{JoinTarget, Page};

#[derive(Debug, Clone)]
pub enum NetMessage {
	ConnectPressed,
	DisconnectPressed,
	ConnectFinished(Result<(), String>),
	NetPolled(Box<Option<crate::net::UiEvent>>),
	AutoJoinCompleted(Vec<(RoomKey, Result<(), String>)>),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum WindowMessage {
	Closed(iced::window::Id),
	Opened(iced::window::Id),
	Resized(iced::window::Id, u32, u32),
	Moved(iced::window::Id, i32, i32),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ChatMessage {
	MessageActionButtonPressed(chatty_domain::RoomKey, Option<String>, Option<String>, Option<String>),
	ReplyToMessage(chatty_domain::RoomKey, Option<String>, Option<String>),
	DeleteMessage(chatty_domain::RoomKey, Option<String>, Option<String>),
	TimeoutUser(chatty_domain::RoomKey, String),
	BanUser(chatty_domain::RoomKey, String),
	Sent(Result<(), String>),
	MessageTextEdit(String, text_editor::Action),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum LayoutMessage {
	ChatLogScrolled(pane_grid::Pane, scrollable::Viewport),
	PaneClicked(pane_grid::Pane),
	PaneResized(pane_grid::ResizeEvent),
	PaneDragged(pane_grid::DragEvent),
	SplitSpiral,
	SplitMasonry,
	SplitPressed,
	CloseFocused,
	NavigatePaneLeft,
	NavigatePaneDown,
	NavigatePaneUp,
	NavigatePaneRight,
}

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum Message {
	Navigate(Page),
	ToasterMessage(crate::app::features::toaster::ToasterMessage),
	OverlayMessage(crate::app::features::overlays::OverlayMessage),
	UsersViewMessage(UsersViewMessage),
	CursorMoved(f32, f32),
	UserScrolled,
	AnimationTick(Instant),

	CharPressed(char, keyboard::Modifiers),
	NamedKeyPressed(iced::keyboard::key::Named),

	ModalDismissed,
	OpenJoinModal(JoinTarget),
	Net(Box<NetMessage>),
	Chat(ChatMessage),

	PaneMessage(pane_grid::Pane, ChatPaneMessage),
	Settings(SettingsMessage),
	PaneSubscribed(pane_grid::Pane, Result<(), String>),
	TabUnsubscribed(RoomKey, Result<(), String>),

	ClipboardRead(crate::app::types::ClipboardTarget, Option<String>),
	Layout(LayoutMessage),
	DismissToast,
	ModifiersChanged(keyboard::Modifiers),

	Window(WindowMessage),
	TabSelected(TabId),
	AddTabPressed,
	CloseTabPressed(TabId),
	PopTab(TabId),
}
