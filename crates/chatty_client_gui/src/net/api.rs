use std::future::Future;
use std::pin::Pin;

use chatty_client_core::{ClientCoreError, SessionControl, SessionEvents};
use chatty_protocol::pb;

pub type BoxedSessionControl = Box<dyn SessionControlApi>;
pub type BoxedSessionEvents = Box<dyn SessionEventsApi>;

pub trait SessionControlApi: Send {
	fn subscribe<'a>(
		&'a mut self,
		subs: Vec<(String, u64)>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>>;

	fn unsubscribe<'a>(
		&'a mut self,
		topics: Vec<String>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>>;

	fn open_events_stream<'a>(
		&'a mut self,
	) -> Pin<Box<dyn Future<Output = Result<BoxedSessionEvents, ClientCoreError>> + Send + 'a>>;

	fn ping<'a>(
		&'a mut self,
		client_time_unix_ms: i64,
	) -> Pin<Box<dyn Future<Output = Result<pb::Pong, ClientCoreError>> + Send + 'a>>;

	fn send_command<'a>(
		&'a mut self,
		command: pb::Command,
	) -> Pin<Box<dyn Future<Output = Result<pb::CommandResult, ClientCoreError>> + Send + 'a>>;

	fn close(&self, code: u32, reason: &str);
}

pub trait SessionEventsApi: Send {
	fn run_events_loop<'a>(
		&'a mut self,
		on_event: Box<dyn FnMut(pb::EventEnvelope) + Send + 'a>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>>;
}

impl SessionControlApi for SessionControl {
	fn subscribe<'a>(
		&'a mut self,
		subs: Vec<(String, u64)>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::subscribe_with_cursors(self, subs).await.map(|_| ()) })
	}

	fn unsubscribe<'a>(
		&'a mut self,
		topics: Vec<String>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::unsubscribe(self, topics).await.map(|_| ()) })
	}

	fn open_events_stream<'a>(
		&'a mut self,
	) -> Pin<Box<dyn Future<Output = Result<BoxedSessionEvents, ClientCoreError>> + Send + 'a>> {
		Box::pin(async move {
			SessionControl::open_events_stream(self)
				.await
				.map(|e| Box::new(e) as BoxedSessionEvents)
		})
	}

	fn ping<'a>(
		&'a mut self,
		client_time_unix_ms: i64,
	) -> Pin<Box<dyn Future<Output = Result<pb::Pong, ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::ping(self, client_time_unix_ms).await })
	}

	fn send_command<'a>(
		&'a mut self,
		command: pb::Command,
	) -> Pin<Box<dyn Future<Output = Result<pb::CommandResult, ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::send_command(self, command).await })
	}

	fn close(&self, code: u32, reason: &str) {
		SessionControl::close(self, code, reason);
	}
}

impl SessionEventsApi for SessionEvents {
	fn run_events_loop<'a>(
		&'a mut self,
		mut on_event: Box<dyn FnMut(pb::EventEnvelope) + Send + 'a>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionEvents::run_events_loop(self, &mut (on_event)).await })
	}
}
