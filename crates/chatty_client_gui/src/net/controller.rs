use chatty_client_core::ClientConfigV1;
use chatty_domain::RoomKey;
use chatty_protocol::pb;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub enum NetCommand {
	Connect {
		cfg: Box<ClientConfigV1>,
	},
	Disconnect {
		reason: String,
	},
	SubscribeRoomKey {
		room: RoomKey,
	},
	UnsubscribeRoomKey {
		room: RoomKey,
	},
	SendCommand {
		command: pb::Command,
	},
}

#[derive(Clone)]
pub struct NetController {
	pub(super) cmd_tx: mpsc::Sender<NetCommand>,
}

impl NetController {
	pub fn new(cmd_tx: mpsc::Sender<NetCommand>) -> Self {
		Self { cmd_tx }
	}

	pub async fn connect(&self, cfg: ClientConfigV1) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::Connect { cfg: Box::new(cfg) })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn disconnect(&self, reason: impl Into<String>) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::Disconnect { reason: reason.into() })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn subscribe_room_key(&self, room: RoomKey) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::SubscribeRoomKey { room })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn unsubscribe_room_key(&self, room: RoomKey) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::UnsubscribeRoomKey { room })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn send_command(&self, command: pb::Command) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::SendCommand { command })
			.await
			.map_err(|_| "network task is not running".to_string())
	}
}

pub struct ShutdownHandle {
	pub(super) _shutdown_tx: oneshot::Sender<()>,
}

impl ShutdownHandle {
	pub fn new(shutdown_tx: oneshot::Sender<()>) -> Self {
		Self {
			_shutdown_tx: shutdown_tx,
		}
	}

	#[allow(dead_code)]
	pub fn shutdown(self) {
		let _ = self._shutdown_tx.send(());
	}
}
