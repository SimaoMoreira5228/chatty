#![forbid(unsafe_code)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chatty_client_core::ClientConfigV1;
use chatty_domain::RoomKey;
use chatty_protocol::pb;

use crate::app::features::layout::UiRootState;
use crate::net::NetController;

pub type NetFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub trait NetEffects: Send + Sync {
	fn connect(&self, cfg: ClientConfigV1) -> NetFuture<Result<(), String>>;
	fn disconnect(&self, reason: String) -> NetFuture<Result<(), String>>;
	fn subscribe_room_key(&self, room: RoomKey) -> NetFuture<Result<(), String>>;
	fn unsubscribe_room_key(&self, room: RoomKey) -> NetFuture<Result<(), String>>;
	fn send_command(&self, command: pb::Command) -> NetFuture<Result<(), String>>;
}

pub trait LayoutStore: Send + Sync {
	fn load(&self) -> Option<UiRootState>;
	fn save(&self, root: &UiRootState);
}

pub trait Clock: Send + Sync {
	fn now(&self) -> std::time::Instant;
}

#[derive(Clone)]
pub struct RealNetEffects {
	net: NetController,
}

impl RealNetEffects {
	pub fn new(net: NetController) -> Self {
		Self { net }
	}
}

impl NetEffects for RealNetEffects {
	fn connect(&self, cfg: ClientConfigV1) -> NetFuture<Result<(), String>> {
		let net = self.net.clone();
		Box::pin(async move { net.connect(cfg).await })
	}

	fn disconnect(&self, reason: String) -> NetFuture<Result<(), String>> {
		let net = self.net.clone();
		Box::pin(async move { net.disconnect(reason).await })
	}

	fn subscribe_room_key(&self, room: RoomKey) -> NetFuture<Result<(), String>> {
		let net = self.net.clone();
		Box::pin(async move { net.subscribe_room_key(room).await })
	}

	fn unsubscribe_room_key(&self, room: RoomKey) -> NetFuture<Result<(), String>> {
		let net = self.net.clone();
		Box::pin(async move { net.unsubscribe_room_key(room).await })
	}

	fn send_command(&self, command: pb::Command) -> NetFuture<Result<(), String>> {
		let net = self.net.clone();
		Box::pin(async move { net.send_command(command).await })
	}
}

#[derive(Clone, Default)]
pub struct FileLayoutStore;

impl LayoutStore for FileLayoutStore {
	fn load(&self) -> Option<UiRootState> {
		crate::app::features::layout::load_ui_layout()
	}

	fn save(&self, root: &UiRootState) {
		crate::app::features::layout::save_ui_layout(root)
	}
}

#[derive(Clone, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
	fn now(&self) -> std::time::Instant {
		std::time::Instant::now()
	}
}

pub type SharedNetEffects = Arc<dyn NetEffects>;
pub type SharedLayoutStore = Arc<dyn LayoutStore>;
pub type SharedClock = Arc<dyn Clock>;
