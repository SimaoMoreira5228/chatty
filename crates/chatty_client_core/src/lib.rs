#![forbid(unsafe_code)]

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use bytes::BytesMut;
use chatty_protocol::framing::{DEFAULT_MAX_FRAME_SIZE, FramingError, encode_frame, try_decode_frame_from_buffer};
use chatty_protocol::pb;
use chatty_util::endpoint::QuicEndpoint;
use quinn::{ClientConfig, Endpoint, TransportConfig, VarInt};
use tokio::io::AsyncWriteExt as _;
use tracing::{debug, info, warn};

mod server_endpoint;

/// Default server endpoint for the standalone client (build-time injection).
pub const DEFAULT_SERVER_ENDPOINT_QUIC: &str = server_endpoint::DEFAULT_SERVER_ENDPOINT;

/// True when the server endpoint is locked (release builds of the standalone client).
pub const SERVER_ENDPOINT_LOCKED: bool = server_endpoint::SERVER_ENDPOINT_LOCKED;

/// Build-time HMAC configuration and keys.
pub const HMAC_ENABLED: bool = server_endpoint::HMAC_ENABLED;
pub const HMAC_KEY: &str = server_endpoint::HMAC_KEY;

/// Build-time external login URLs (may be empty).
pub const TWITCH_LOGIN_URL: &str = server_endpoint::TWITCH_LOGIN_URL;
pub const KICK_LOGIN_URL: &str = server_endpoint::KICK_LOGIN_URL;

/// Current protocol version used in `pb::Envelope.version`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Client session configuration (v1).
#[derive(Debug, Clone)]
pub struct ClientConfigV1 {
	/// Remote server host (DNS name or IP literal).
	pub server_host: String,

	/// Remote server UDP port.
	pub server_port: u16,

	/// Resolved remote server address override.
	pub server_addr: Option<SocketAddr>,

	/// Client identifier.
	pub client_name: String,

	/// Client instance id.
	pub client_instance_id: String,

	/// Optional auth token for server access.
	pub auth_token: Option<String>,

	/// Optional user OAuth token (e.g. Twitch).
	pub user_oauth_token: Option<String>,

	/// Optional Twitch client id (required by Twitch APIs).
	pub twitch_client_id: Option<String>,

	/// Optional Twitch user id.
	pub twitch_user_id: Option<String>,

	/// Optional Twitch username/login.
	pub twitch_username: Option<String>,

	/// Optional Kick user OAuth token.
	pub kick_user_oauth_token: Option<String>,

	/// Optional Kick user id.
	pub kick_user_id: Option<String>,

	/// Optional Kick username/login.
	pub kick_username: Option<String>,

	/// Maximum inbound/outbound frame size.
	pub max_frame_bytes: usize,

	/// Timeout for connect + handshake.
	pub connect_timeout: Duration,
}

impl ClientConfigV1 {
	/// Returns the injected/default server endpoint string in `quic://host:port` form.
	pub fn default_server_endpoint_quic() -> &'static str {
		DEFAULT_SERVER_ENDPOINT_QUIC
	}

	/// Returns true when the server endpoint is locked at build time.
	pub fn server_endpoint_locked() -> bool {
		SERVER_ENDPOINT_LOCKED
	}

	/// Parse a `quic://host:port` endpoint into `(host, port)`.
	pub fn parse_quic_endpoint(endpoint: &str) -> Result<(String, u16), ClientCoreError> {
		let e = QuicEndpoint::parse(endpoint)
			.map_err(|msg| ClientCoreError::Protocol(format!("invalid endpoint (expected quic://host:port): {msg}")))?;
		Ok((e.host, e.port))
	}

	/// Convenience: create a config from `quic://host:port`.
	pub fn from_quic_endpoint(endpoint: &str) -> Result<Self, ClientCoreError> {
		let (host, port) = Self::parse_quic_endpoint(endpoint)?;
		Ok(Self {
			server_host: host,
			server_port: port,
			server_addr: None,
			..Self::default()
		})
	}
}

impl Default for ClientConfigV1 {
	fn default() -> Self {
		// Local dev default. Release builds should use build-time injection.
		Self {
			server_host: "localhost".to_string(),
			server_port: 18203,
			server_addr: Some("127.0.0.1:18203".parse().expect("valid default addr")),
			client_name: format!("chatty-client-core/{}", env!("CARGO_PKG_VERSION")),
			client_instance_id: "dev-instance".to_string(),
			auth_token: None,
			user_oauth_token: None,
			twitch_client_id: None,
			twitch_user_id: None,
			twitch_username: None,
			kick_user_oauth_token: None,
			kick_user_id: None,
			kick_username: None,
			max_frame_bytes: DEFAULT_MAX_FRAME_SIZE,
			connect_timeout: Duration::from_secs(15),
		}
	}
}

/// Errors for client core operations.
#[derive(Debug, thiserror::Error)]
pub enum ClientCoreError {
	/// QUIC endpoint setup failed.
	#[error("failed to create QUIC endpoint: {0}")]
	Endpoint(String),

	/// Connection establishment failed.
	#[error("failed to connect: {0}")]
	Connect(String),

	/// Protocol framing error.
	#[error(transparent)]
	Framing(#[from] FramingError),

	/// Protocol error (unexpected message ordering/types).
	#[error("protocol error: {0}")]
	Protocol(String),

	/// IO error.
	#[error("io error: {0}")]
	Io(String),

	/// Other error.
	#[error("error: {0}")]
	Other(String),
}

impl From<anyhow::Error> for ClientCoreError {
	fn from(e: anyhow::Error) -> Self {
		ClientCoreError::Other(format!("{e:#}"))
	}
}

/// Control half of a session (subscribe/unsubscribe, close).
pub struct SessionControl {
	conn: quinn::Connection,
	control_send: quinn::SendStream,
	control_recv: quinn::RecvStream,
	max_frame_bytes: usize,
	events_opened: bool,
}

/// Events reader half of a session.
pub struct SessionEvents {
	events_recv: quinn::RecvStream,
	// Keep the send half alive so the peer doesn't see an immediate FIN.
	_events_send_keepalive: quinn::SendStream,
	max_frame_bytes: usize,
}

impl SessionControl {
	/// Connect and perform the v1 handshake.
	pub async fn connect(cfg: ClientConfigV1) -> Result<(Self, pb::Welcome), ClientCoreError> {
		let endpoint = make_client_endpoint().map_err(|e| ClientCoreError::Endpoint(format!("{e:#}")))?;

		let quinn_cfg = make_insecure_client_config().map_err(|e| ClientCoreError::Endpoint(format!("{e:#}")))?;

		let connect_timeout = cfg.connect_timeout;

		let server_name = cfg.server_host.clone();

		let candidates: Vec<SocketAddr> = match cfg.server_addr {
			Some(addr) => vec![addr],
			None => {
				let hostport = format!("{}:{}", cfg.server_host, cfg.server_port);
				let addrs = hostport
					.to_socket_addrs()
					.map_err(|e| ClientCoreError::Connect(format!("failed to resolve {hostport}: {e}")))?;

				let addrs: Vec<SocketAddr> = addrs.collect();
				if addrs.is_empty() {
					return Err(ClientCoreError::Connect(format!(
						"DNS resolution returned no addresses for {hostport}"
					)));
				}
				addrs
			}
		};

		let mut last_err: Option<String> = None;
		let mut conn: Option<quinn::Connection> = None;

		for server_addr in candidates {
			let connecting = endpoint
				.connect_with(quinn_cfg.clone(), server_addr, &server_name)
				.map_err(|e| ClientCoreError::Connect(format!("connect_with({server_addr}, sni={server_name}): {e}")))?;

			match tokio::time::timeout(connect_timeout, connecting).await {
				Ok(Ok(c)) => {
					conn = Some(c);
					break;
				}
				Ok(Err(e)) => {
					last_err = Some(format!("connect failed (addr={server_addr}, sni={server_name}): {e}"));
				}
				Err(_) => {
					last_err = Some(format!(
						"connect timeout after {connect_timeout:?} (addr={server_addr}, sni={server_name})"
					));
				}
			}
		}

		let conn = conn.ok_or_else(|| {
			ClientCoreError::Connect(
				last_err.unwrap_or_else(|| format!("connect failed (no addresses attempted) (sni={server_name})")),
			)
		})?;

		info!(remote = %conn.remote_address(), "connected");

		let (mut control_send, mut control_recv) = tokio::time::timeout(connect_timeout, conn.open_bi())
			.await
			.map_err(|_| ClientCoreError::Io(format!("timeout opening control stream after {connect_timeout:?}")))?
			.map_err(|e| ClientCoreError::Io(format!("open_bi(control) failed: {e}")))?;

		let hello = pb::Hello {
			client_name: cfg.client_name,
			client_instance_id: cfg.client_instance_id,
			auth_token: cfg.auth_token.unwrap_or_default(),
			user_oauth_token: cfg.user_oauth_token.unwrap_or_default(),
			twitch_client_id: cfg.twitch_client_id.unwrap_or_default(),
			twitch_user_id: cfg.twitch_user_id.unwrap_or_default(),
			twitch_username: cfg.twitch_username.unwrap_or_default(),
			kick_user_oauth_token: cfg.kick_user_oauth_token.unwrap_or_default(),
			kick_user_id: cfg.kick_user_id.unwrap_or_default(),
			kick_username: cfg.kick_username.unwrap_or_default(),
			supported_codecs: vec![pb::Codec::Protobuf as i32],
			preferred_codec: pb::Codec::Protobuf as i32,
		};
		let env = pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Hello(hello)),
		};
		write_envelope(&mut control_send, &env, cfg.max_frame_bytes)
			.await
			.map_err(|e| ClientCoreError::Io(format!("send Hello failed: {e}")))?;

		let welcome_env = tokio::time::timeout(connect_timeout, read_one_envelope(&mut control_recv, cfg.max_frame_bytes))
			.await
			.map_err(|_| ClientCoreError::Protocol(format!("timeout waiting for Welcome after {connect_timeout:?}")))??;

		let welcome = match welcome_env.msg {
			Some(pb::envelope::Msg::Welcome(w)) => w,
			other => {
				return Err(ClientCoreError::Protocol(format!("expected Welcome, got {other:?}")));
			}
		};

		if welcome.selected_codec != 0 && welcome.selected_codec != (pb::Codec::Protobuf as i32) {
			return Err(ClientCoreError::Protocol(format!(
				"unsupported negotiated codec: {}",
				welcome.selected_codec
			)));
		}

		debug!(
			server_name = %welcome.server_name,
			server_instance_id = %welcome.server_instance_id,
			max_frame_bytes = welcome.max_frame_bytes,
			"received Welcome"
		);

		let control = Self {
			conn,
			control_send,
			control_recv,
			max_frame_bytes: (welcome.max_frame_bytes as usize).min(cfg.max_frame_bytes),
			events_opened: false,
		};

		Ok((control, welcome))
	}

	/// Subscribe to topics with optional resume cursors.
	pub async fn subscribe_with_cursors(
		&mut self,
		subs: impl IntoIterator<Item = (String, u64)>,
	) -> Result<pb::Subscribed, ClientCoreError> {
		let subs = subs
			.into_iter()
			.map(|(topic, last_cursor)| pb::Subscription { topic, last_cursor })
			.collect();

		debug!(subs = ?subs, "sending subscribe");

		let env = pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Subscribe(pb::Subscribe { subs })),
		};

		write_envelope(&mut self.control_send, &env, self.max_frame_bytes).await?;

		let resp = read_one_envelope(&mut self.control_recv, self.max_frame_bytes).await?;
		match resp.msg {
			Some(pb::envelope::Msg::Subscribed(s)) => {
				debug!("subscribe acknowledged");
				Ok(s)
			}
			other => Err(ClientCoreError::Protocol(format!("expected Subscribed, got {other:?}"))),
		}
	}

	/// Subscribe to topics.
	pub async fn subscribe(&mut self, topics: impl IntoIterator<Item = String>) -> Result<pb::Subscribed, ClientCoreError> {
		self.subscribe_with_cursors(topics.into_iter().map(|topic| (topic, 0))).await
	}

	/// Unsubscribe from topics.
	pub async fn unsubscribe(
		&mut self,
		topics: impl IntoIterator<Item = String>,
	) -> Result<pb::Unsubscribed, ClientCoreError> {
		let topics_vec: Vec<String> = topics.into_iter().collect();
		debug!(topics = ?topics_vec, "sending unsubscribe");

		let env = pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Unsubscribe(pb::Unsubscribe { topics: topics_vec })),
		};

		write_envelope(&mut self.control_send, &env, self.max_frame_bytes).await?;

		let resp = read_one_envelope(&mut self.control_recv, self.max_frame_bytes).await?;
		match resp.msg {
			Some(pb::envelope::Msg::Unsubscribed(u)) => {
				debug!("unsubscribe acknowledged");
				Ok(u)
			}
			other => Err(ClientCoreError::Protocol(format!("expected Unsubscribed, got {other:?}"))),
		}
	}

	/// Send a command to the server (future send/mod flows).
	pub async fn send_command(&mut self, command: pb::Command) -> Result<pb::CommandResult, ClientCoreError> {
		let env = pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Command(command)),
		};

		write_envelope(&mut self.control_send, &env, self.max_frame_bytes).await?;

		let resp = read_one_envelope(&mut self.control_recv, self.max_frame_bytes).await?;
		match resp.msg {
			Some(pb::envelope::Msg::CommandResult(r)) => Ok(r),
			other => Err(ClientCoreError::Protocol(format!("expected CommandResult, got {other:?}"))),
		}
	}

	/// Send a keepalive ping and await the pong response.
	pub async fn ping(&mut self, client_time_unix_ms: i64) -> Result<pb::Pong, ClientCoreError> {
		let env = pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Ping(pb::Ping { client_time_unix_ms })),
		};

		write_envelope(&mut self.control_send, &env, self.max_frame_bytes).await?;

		let resp = read_one_envelope(&mut self.control_recv, self.max_frame_bytes).await?;
		match resp.msg {
			Some(pb::envelope::Msg::Pong(p)) => Ok(p),
			other => Err(ClientCoreError::Protocol(format!("expected Pong, got {other:?}"))),
		}
	}

	/// Open the events stream after a successful subscribe.
	pub async fn open_events_stream(&mut self) -> Result<SessionEvents, ClientCoreError> {
		if self.events_opened {
			return Err(ClientCoreError::Protocol(
				"events stream already opened; reuse the existing SessionEvents".to_string(),
			));
		}

		debug!("open_events_stream(): opening events stream (client open_bi)");
		let (mut send, recv) = self
			.conn
			.open_bi()
			.await
			.map_err(|e| ClientCoreError::Io(format!("open_bi(events) failed: {e}")))?;
		debug!("open_events_stream(): opened events stream (client open_bi succeeded)");

		// Force a STREAM frame so the server observes the stream promptly.
		send.write_all(&[0u8])
			.await
			.map_err(|e| ClientCoreError::Io(format!("failed to write events stream activation byte: {e}")))?;
		send.flush()
			.await
			.map_err(|e| ClientCoreError::Io(format!("failed to flush events stream activation byte: {e}")))?;

		self.events_opened = true;

		Ok(SessionEvents {
			events_recv: recv,
			_events_send_keepalive: send,
			max_frame_bytes: self.max_frame_bytes,
		})
	}

	pub fn close(&self, code: u32, reason: &str) {
		self.conn.close(quinn::VarInt::from_u32(code), reason.as_bytes());
	}
}

impl SessionEvents {
	/// Run the events loop until EOF or error.
	pub async fn run_events_loop<F>(&mut self, mut on_event: F) -> Result<(), ClientCoreError>
	where
		F: FnMut(pb::EventEnvelope),
	{
		let mut buf = BytesMut::with_capacity(16 * 1024);
		let mut tmp = [0u8; 8192];

		loop {
			let n = match self.events_recv.read(&mut tmp).await {
				Ok(Some(n)) => n,
				Ok(None) => {
					info!("events stream closed");
					return Ok(());
				}
				Err(e) => return Err(ClientCoreError::Io(e.to_string())),
			};

			buf.extend_from_slice(&tmp[..n]);

			loop {
				match try_decode_frame_from_buffer::<pb::Envelope>(&mut buf, self.max_frame_bytes) {
					Ok(Some(env)) => {
						if let Some(msg) = env.msg {
							match msg {
								pb::envelope::Msg::Event(ev) => {
									debug!(
										topic = %ev.topic,
										cursor = ev.cursor,
										event_kind = %event_kind(&ev),
										"events stream decoded"
									);
									on_event(ev)
								}
								other => warn!("unexpected message on events stream: {:?}", other),
							}
						}
					}
					Ok(None) => break,
					Err(e) => return Err(ClientCoreError::Framing(e)),
				}
			}
		}
	}
}

async fn write_envelope(
	send: &mut quinn::SendStream,
	env: &pb::Envelope,
	max_frame_bytes: usize,
) -> Result<(), ClientCoreError> {
	let frame = encode_frame(env, max_frame_bytes).map_err(ClientCoreError::Framing)?;
	send.write_all(&frame).await.map_err(|e| ClientCoreError::Io(e.to_string()))?;
	send.flush().await.map_err(|e| ClientCoreError::Io(e.to_string()))?;
	Ok(())
}

fn event_kind(ev: &pb::EventEnvelope) -> &'static str {
	match ev.event.as_ref() {
		Some(pb::event_envelope::Event::ChatMessage(_)) => "chat_message",
		Some(pb::event_envelope::Event::TopicLagged(_)) => "topic_lagged",
		Some(pb::event_envelope::Event::Permissions(_)) => "permissions",
		Some(pb::event_envelope::Event::AssetBundle(_)) => "asset_bundle",
		None => "empty",
	}
}

async fn read_one_envelope(recv: &mut quinn::RecvStream, max_frame_bytes: usize) -> Result<pb::Envelope, ClientCoreError> {
	let mut buf = BytesMut::with_capacity(8 * 1024);
	let mut tmp = [0u8; 8192];

	loop {
		// Try decoding first in case buffer already has a full frame.
		match try_decode_frame_from_buffer::<pb::Envelope>(&mut buf, max_frame_bytes) {
			Ok(Some(env)) => return Ok(env),
			Ok(None) => {}
			Err(e) => return Err(ClientCoreError::Framing(e)),
		}

		let n = match recv.read(&mut tmp).await {
			Ok(Some(n)) => n,
			Ok(None) => {
				return Err(ClientCoreError::Protocol(
					"stream closed before receiving full message".to_string(),
				));
			}
			Err(e) => return Err(ClientCoreError::Io(e.to_string())),
		};

		buf.extend_from_slice(&tmp[..n]);
	}
}

fn make_client_endpoint() -> anyhow::Result<Endpoint> {
	let addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
	let endpoint = Endpoint::client(addr).context("create client endpoint")?;
	Ok(endpoint)
}

/// Dev-only TLS config that skips server cert validation.
fn make_insecure_client_config() -> anyhow::Result<ClientConfig> {
	let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

	#[derive(Debug)]
	struct NoVerifier;

	impl rustls::client::danger::ServerCertVerifier for NoVerifier {
		fn verify_server_cert(
			&self,
			_end_entity: &rustls::pki_types::CertificateDer<'_>,
			_intermediates: &[rustls::pki_types::CertificateDer<'_>],
			_server_name: &rustls::pki_types::ServerName<'_>,
			_ocsp_response: &[u8],
			_now: rustls::pki_types::UnixTime,
		) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
			let _ = _intermediates;
			Ok(rustls::client::danger::ServerCertVerified::assertion())
		}

		fn verify_tls12_signature(
			&self,
			_message: &[u8],
			_cert: &rustls::pki_types::CertificateDer<'_>,
			_dss: &rustls::DigitallySignedStruct,
		) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
			Err(rustls::Error::General("TLS1.2 not supported".into()))
		}

		fn verify_tls13_signature(
			&self,
			_message: &[u8],
			_cert: &rustls::pki_types::CertificateDer<'_>,
			_dss: &rustls::DigitallySignedStruct,
		) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
			Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
		}

		fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
			vec![
				rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
				rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
				rustls::SignatureScheme::RSA_PSS_SHA256,
				rustls::SignatureScheme::RSA_PSS_SHA384,
				rustls::SignatureScheme::RSA_PSS_SHA512,
				rustls::SignatureScheme::ED25519,
			]
		}
	}

	let mut tls = rustls::ClientConfig::builder()
		.with_root_certificates(rustls::RootCertStore::empty())
		.with_no_client_auth();

	tls.dangerous().set_certificate_verifier(Arc::new(NoVerifier));
	tls.alpn_protocols = vec![b"chatty-v1".to_vec()];

	let quic_tls = quinn::crypto::rustls::QuicClientConfig::try_from(tls)?;

	let mut cfg = ClientConfig::new(Arc::new(quic_tls));

	// Allow multiple streams (control + events at minimum).
	let mut transport = TransportConfig::default();
	transport.max_concurrent_bidi_streams(VarInt::from_u32(64));
	transport.max_concurrent_uni_streams(VarInt::from_u32(64));
	cfg.transport_config(Arc::new(transport));

	Ok(cfg)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_config_is_sane() {
		let cfg = ClientConfigV1::default();
		assert_eq!(cfg.server_host, "localhost");
		assert!(cfg.max_frame_bytes > 0);
	}
}
