#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::warn;

#[derive(Clone, Default)]
pub struct HealthState {
	ready: Arc<AtomicBool>,
}

impl HealthState {
	pub fn new() -> Self {
		Self {
			ready: Arc::new(AtomicBool::new(false)),
		}
	}

	pub fn mark_ready(&self) {
		self.ready.store(true, Ordering::Relaxed);
	}

	pub fn is_ready(&self) -> bool {
		self.ready.load(Ordering::Relaxed)
	}
}

pub fn spawn_health_server(bind: SocketAddr, state: HealthState) {
	tokio::spawn(async move {
		if let Err(err) = run_health_server(bind, state).await {
			warn!(error = %err, "health server stopped");
		}
	});
}

async fn run_health_server(bind: SocketAddr, state: HealthState) -> anyhow::Result<()> {
	let listener = TcpListener::bind(bind).await?;
	loop {
		let (stream, _addr) = listener.accept().await?;
		let io = TokioIo::new(stream);
		let state = state.clone();
		tokio::spawn(async move {
			let service = service_fn(move |req| handle_health(req, state.clone()));
			if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
				warn!(error = %err, "health connection error");
			}
		});
	}
}

async fn handle_health(req: Request<Incoming>, state: HealthState) -> Result<Response<Full<Bytes>>, hyper::Error> {
	if req.method() != Method::GET {
		return Ok(Response::builder()
			.status(StatusCode::METHOD_NOT_ALLOWED)
			.body(Full::new(Bytes::new()))
			.unwrap());
	}

	let path = req.uri().path();
	match path {
		"/healthz" => Ok(Response::builder()
			.status(StatusCode::OK)
			.body(Full::new(Bytes::from_static(b"ok")))
			.unwrap()),
		"/readyz" => {
			if state.is_ready() {
				Ok(Response::builder()
					.status(StatusCode::OK)
					.body(Full::new(Bytes::from_static(b"ready")))
					.unwrap())
			} else {
				Ok(Response::builder()
					.status(StatusCode::SERVICE_UNAVAILABLE)
					.body(Full::new(Bytes::from_static(b"not-ready")))
					.unwrap())
			}
		}
		_ => Ok(Response::builder()
			.status(StatusCode::NOT_FOUND)
			.body(Full::new(Bytes::new()))
			.unwrap()),
	}
}
