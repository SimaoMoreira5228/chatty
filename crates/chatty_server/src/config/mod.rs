#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, anyhow};
use chatty_platform::SecretString;
use serde::Deserialize;
use tracing::{debug, info, warn};

/// Default config path: `~/.chatty/config.toml`.
pub fn default_config_path() -> anyhow::Result<PathBuf> {
	let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
	Ok(home.join(".chatty").join("config.toml"))
}

/// Load the server config from TOML and env overrides.
#[allow(dead_code)]
pub fn load_server_config() -> anyhow::Result<ServerConfig> {
	let path = default_config_path()?;
	load_server_config_from_path(&path)
}

/// Same as `load_server_config` but with an explicit config path.
pub fn load_server_config_from_path(path: &Path) -> anyhow::Result<ServerConfig> {
	let file_cfg = read_toml_if_exists(path)
		.with_context(|| format!("read config from {}", path.display()))?
		.unwrap_or_default();

	let mut cfg = ServerConfig::from_file(file_cfg);

	apply_env_overrides(&mut cfg);

	Ok(cfg)
}

/// Server config (v1).
#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
	pub auth_token: Option<SecretString>,
	pub server: ServerSettings,
	pub twitch: TwitchSettings,
	pub kick: KickSettings,
	pub persistence: PersistenceSettings,
}

/// Server settings loaded by the server.
#[derive(Debug, Clone, Default)]
pub struct ServerSettings {
	/// PEM-encoded certificate path for QUIC/TLS.
	pub tls_cert_path: Option<PathBuf>,
	/// PEM-encoded private key path for QUIC/TLS.
	pub tls_key_path: Option<PathBuf>,
	/// Optional metrics exporter bind address (host:port).
	pub metrics_bind: Option<String>,
	/// Optional health/readiness HTTP bind address (host:port).
	pub health_bind: Option<String>,
	/// HMAC secret for stateless access tokens.
	pub auth_hmac_secret: Option<SecretString>,
	/// Command rate limiting: per-connection burst size.
	pub command_rate_limit_per_conn_burst: u32,
	/// Command rate limiting: per-connection requests per minute.
	pub command_rate_limit_per_conn_per_minute: u32,
	/// Command rate limiting: per-topic burst size.
	pub command_rate_limit_per_topic_burst: u32,
	/// Command rate limiting: per-topic requests per minute.
	pub command_rate_limit_per_topic_per_minute: u32,
}

/// Persistence settings loaded by the server.
#[derive(Debug, Clone, Default)]
pub struct PersistenceSettings {
	/// Enable persistence.
	pub enabled: bool,
	/// Database URL (sqlite: or postgres:).
	pub database_url: Option<String>,
	/// Enable replay (cursor resume) with configured backend.
	pub replay_enabled: bool,
	/// Per-topic replay capacity (optional override).
	pub replay_capacity: Option<usize>,
	/// Optional retention window (minutes) for replay events.
	pub replay_retention_minutes: Option<u64>,
}

/// Twitch settings loaded by the server.
#[derive(Debug, Clone, Default)]
pub struct TwitchSettings {
	/// Twitch App Client ID.
	pub client_id: Option<String>,
	/// Twitch App Client Secret (optional; used for token refresh).
	pub client_secret: Option<SecretString>,

	/// Disable automatic refresh; expiry becomes a hard error.
	pub disable_refresh: bool,

	/// Twitch user access token (bearer).
	pub user_access_token: Option<SecretString>,
	/// Optional refresh token for user access token.
	pub refresh_token: Option<SecretString>,

	/// EventSub websocket URL (optional override).
	pub eventsub_ws_url: Option<String>,

	/// Reconnect backoff min/max (optional).
	pub reconnect_min_delay: Option<Duration>,
	pub reconnect_max_delay: Option<Duration>,

	/// Refresh buffer before expiry.
	pub refresh_buffer: Option<Duration>,

	/// Optional overrides: room/login -> broadcaster id (string).
	pub broadcaster_id_overrides: BTreeMap<String, String>,
}

/// Kick settings loaded by the server.
#[derive(Debug, Clone, Default)]
pub struct KickSettings {
	/// Kick API base URL.
	pub base_url: Option<String>,
	/// Kick Pusher websocket URL override.
	pub pusher_ws_url: Option<String>,
	/// Reconnect backoff min/max (optional).
	pub reconnect_min_delay: Option<Duration>,
	pub reconnect_max_delay: Option<Duration>,
	/// Optional overrides: channel slug -> broadcaster id.
	pub broadcaster_id_overrides: BTreeMap<String, String>,
}

impl TwitchSettings {
	/// Whether Twitch ingestion should be enabled (v1).
	#[allow(dead_code)]
	pub fn is_configured(&self) -> bool {
		true
	}
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileConfig {
	auth_token: Option<String>,

	#[serde(default)]
	server: FileServerSettings,

	#[serde(default)]
	twitch: FileTwitchSettings,

	#[serde(default)]
	kick: FileKickSettings,

	#[serde(default)]
	persistence: FilePersistenceSettings,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileServerSettings {
	tls_cert_path: Option<String>,
	tls_key_path: Option<String>,
	metrics_bind: Option<String>,
	health_bind: Option<String>,
	auth_hmac_secret: Option<String>,
	command_rate_limit_per_conn_burst: Option<u32>,
	command_rate_limit_per_conn_per_minute: Option<u32>,
	command_rate_limit_per_topic_burst: Option<u32>,
	command_rate_limit_per_topic_per_minute: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FilePersistenceSettings {
	enabled: Option<bool>,
	database_url: Option<String>,
	replay_enabled: Option<bool>,
	replay_capacity: Option<usize>,
	replay_retention_minutes: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileTwitchSettings {
	client_id: Option<String>,
	client_secret: Option<String>,
	user_access_token: Option<String>,
	refresh_token: Option<String>,
	disable_refresh: Option<bool>,
	eventsub_ws_url: Option<String>,

	reconnect_min_delay_ms: Option<u64>,
	reconnect_max_delay_ms: Option<u64>,
	refresh_buffer_secs: Option<u64>,

	#[serde(default)]
	broadcaster_id_overrides: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileKickSettings {
	base_url: Option<String>,
	pusher_ws_url: Option<String>,
	reconnect_min_delay_ms: Option<u64>,
	reconnect_max_delay_ms: Option<u64>,

	#[serde(default)]
	broadcaster_id_overrides: BTreeMap<String, String>,
}

impl ServerConfig {
	fn from_file(file: FileConfig) -> Self {
		let twitch = TwitchSettings {
			client_id: file.twitch.client_id.filter(|s| !s.trim().is_empty()),
			client_secret: file
				.twitch
				.client_secret
				.filter(|s| !s.trim().is_empty())
				.map(SecretString::new),
			disable_refresh: file.twitch.disable_refresh.unwrap_or(false),
			user_access_token: file
				.twitch
				.user_access_token
				.filter(|s| !s.trim().is_empty())
				.map(SecretString::new),
			refresh_token: file
				.twitch
				.refresh_token
				.filter(|s| !s.trim().is_empty())
				.map(SecretString::new),
			eventsub_ws_url: file.twitch.eventsub_ws_url.filter(|s| !s.trim().is_empty()),
			reconnect_min_delay: file.twitch.reconnect_min_delay_ms.map(Duration::from_millis),
			reconnect_max_delay: file.twitch.reconnect_max_delay_ms.map(Duration::from_millis),
			refresh_buffer: file.twitch.refresh_buffer_secs.map(Duration::from_secs),
			broadcaster_id_overrides: file.twitch.broadcaster_id_overrides,
		};

		let kick = KickSettings {
			base_url: file.kick.base_url.filter(|s| !s.trim().is_empty()),
			pusher_ws_url: file.kick.pusher_ws_url.filter(|s| !s.trim().is_empty()),
			reconnect_min_delay: file.kick.reconnect_min_delay_ms.map(Duration::from_millis),
			reconnect_max_delay: file.kick.reconnect_max_delay_ms.map(Duration::from_millis),
			broadcaster_id_overrides: file.kick.broadcaster_id_overrides,
		};

		let replay_retention_minutes = file.persistence.replay_retention_minutes.filter(|v| *v > 0);

		Self {
			auth_token: file.auth_token.filter(|s| !s.trim().is_empty()).map(SecretString::new),
			server: ServerSettings {
				tls_cert_path: file.server.tls_cert_path.filter(|s| !s.trim().is_empty()).map(PathBuf::from),
				tls_key_path: file.server.tls_key_path.filter(|s| !s.trim().is_empty()).map(PathBuf::from),
				metrics_bind: file.server.metrics_bind.filter(|s| !s.trim().is_empty()),
				health_bind: file.server.health_bind.filter(|s| !s.trim().is_empty()),
				auth_hmac_secret: file
					.server
					.auth_hmac_secret
					.filter(|s| !s.trim().is_empty())
					.map(SecretString::new),
				command_rate_limit_per_conn_burst: file.server.command_rate_limit_per_conn_burst.unwrap_or(20),
				command_rate_limit_per_conn_per_minute: file.server.command_rate_limit_per_conn_per_minute.unwrap_or(120),
				command_rate_limit_per_topic_burst: file.server.command_rate_limit_per_topic_burst.unwrap_or(10),
				command_rate_limit_per_topic_per_minute: file.server.command_rate_limit_per_topic_per_minute.unwrap_or(60),
			},
			twitch,
			kick,
			persistence: PersistenceSettings {
				enabled: file.persistence.enabled.unwrap_or(false),
				database_url: file.persistence.database_url.filter(|s| !s.trim().is_empty()),
				replay_enabled: file.persistence.replay_enabled.unwrap_or(false),
				replay_capacity: file.persistence.replay_capacity,
				replay_retention_minutes,
			},
		}
	}
}

fn parse_env_bool(v: &str) -> Option<bool> {
	match v.trim().to_ascii_lowercase().as_str() {
		"1" | "true" | "yes" | "on" => Some(true),
		"0" | "false" | "no" | "off" => Some(false),
		_ => None,
	}
}

fn read_toml_if_exists(path: &Path) -> anyhow::Result<Option<FileConfig>> {
	match fs::read_to_string(path) {
		Ok(s) => {
			let cfg: FileConfig = toml::from_str(&s).context("parse TOML")?;
			Ok(Some(cfg))
		}
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(anyhow!(e).context("read config file")),
	}
}

fn apply_env_overrides(cfg: &mut ServerConfig) {
	if let Ok(v) = std::env::var("CHATTY_SERVER_AUTH_TOKEN") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.auth_token = Some(SecretString::new(v));
			info!("server auth: auth_token overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_SERVER_TLS_CERT") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.server.tls_cert_path = Some(PathBuf::from(v));
			info!("server config: tls_cert_path overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_SERVER_TLS_KEY") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.server.tls_key_path = Some(PathBuf::from(v));
			info!("server config: tls_key_path overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_SERVER_AUTH_HMAC_SECRET") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.server.auth_hmac_secret = Some(SecretString::new(v));
			info!("server auth: auth_hmac_secret overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_COMMAND_RATE_LIMIT_PER_CONN_BURST")
		&& let Ok(burst) = v.trim().parse::<u32>()
	{
		cfg.server.command_rate_limit_per_conn_burst = burst;
		info!(burst, "server config: command_rate_limit_per_conn_burst overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_COMMAND_RATE_LIMIT_PER_CONN_PER_MINUTE")
		&& let Ok(rate) = v.trim().parse::<u32>()
	{
		cfg.server.command_rate_limit_per_conn_per_minute = rate;
		info!(
			rate,
			"server config: command_rate_limit_per_conn_per_minute overridden by env"
		);
	}

	if let Ok(v) = std::env::var("CHATTY_COMMAND_RATE_LIMIT_PER_TOPIC_BURST")
		&& let Ok(burst) = v.trim().parse::<u32>()
	{
		cfg.server.command_rate_limit_per_topic_burst = burst;
		info!(burst, "server config: command_rate_limit_per_topic_burst overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_COMMAND_RATE_LIMIT_PER_TOPIC_PER_MINUTE")
		&& let Ok(rate) = v.trim().parse::<u32>()
	{
		cfg.server.command_rate_limit_per_topic_per_minute = rate;
		info!(
			rate,
			"server config: command_rate_limit_per_topic_per_minute overridden by env"
		);
	}

	if let Ok(v) = std::env::var("CHATTY_KICK_BASE_URL") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.kick.base_url = Some(v);
			info!("kick config: base_url overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_KICK_PUSHER_WS_URL") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.kick.pusher_ws_url = Some(v);
			info!("kick config: pusher_ws_url overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_KICK_RECONNECT_MIN_DELAY_MS")
		&& let Ok(min_ms) = v.trim().parse::<u64>()
	{
		cfg.kick.reconnect_min_delay = Some(Duration::from_millis(min_ms));
		info!(min_ms, "kick config: reconnect_min_delay overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_KICK_RECONNECT_MAX_DELAY_MS")
		&& let Ok(max_ms) = v.trim().parse::<u64>()
	{
		cfg.kick.reconnect_max_delay = Some(Duration::from_millis(max_ms));
		info!(max_ms, "kick config: reconnect_max_delay overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_METRICS_BIND") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.server.metrics_bind = Some(v);
			info!("server config: metrics_bind overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_HEALTH_BIND") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.server.health_bind = Some(v);
			info!("server config: health_bind overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_PERSISTENCE_ENABLED")
		&& let Some(enabled) = parse_env_bool(&v)
	{
		cfg.persistence.enabled = enabled;
		info!(enabled, "persistence: enabled overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_PERSISTENCE_DATABASE_URL") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.persistence.database_url = Some(v);
			info!("persistence: database_url overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_REPLAY_ENABLED")
		&& let Some(enabled) = parse_env_bool(&v)
	{
		cfg.persistence.replay_enabled = enabled;
		info!(enabled, "persistence: replay_enabled overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_REPLAY_CAPACITY")
		&& let Ok(capacity) = v.trim().parse::<usize>()
	{
		cfg.persistence.replay_capacity = Some(capacity);
		info!(capacity, "persistence: replay_capacity overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_REPLAY_RETENTION_MINUTES")
		&& let Ok(retention) = v.trim().parse::<u64>()
	{
		cfg.persistence.replay_retention_minutes = Some(retention);
		info!(retention, "persistence: replay_retention_minutes overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_CLIENT_ID") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.twitch.client_id = Some(v);
			info!("twitch config: client_id overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_CLIENT_SECRET") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.twitch.client_secret = Some(SecretString::new(v));
			info!("twitch config: client_secret overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_DISABLE_REFRESH")
		&& let Some(disable) = parse_env_bool(&v)
	{
		cfg.twitch.disable_refresh = disable;
		info!(disable_refresh = disable, "twitch config: disable_refresh overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_USER_ACCESS_TOKEN") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.twitch.user_access_token = Some(SecretString::new(v));
			info!("twitch config: user_access_token overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_REFRESH_TOKEN") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.twitch.refresh_token = Some(SecretString::new(v));
			info!("twitch config: refresh_token overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_EVENTSUB_WS_URL") {
		let v = v.trim().to_string();
		if !v.is_empty() {
			cfg.twitch.eventsub_ws_url = Some(v);
			info!("twitch config: eventsub_ws_url overridden by env");
		}
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_RECONNECT_MIN_DELAY_MS")
		&& let Ok(ms) = v.trim().parse::<u64>()
	{
		cfg.twitch.reconnect_min_delay = Some(Duration::from_millis(ms));
		debug!("twitch config: reconnect_min_delay overridden by env");
	}

	if let Ok(v) = std::env::var("CHATTY_TWITCH_RECONNECT_MAX_DELAY_MS")
		&& let Ok(ms) = v.trim().parse::<u64>()
	{
		cfg.twitch.reconnect_max_delay = Some(Duration::from_millis(ms));
		debug!("twitch config: reconnect_max_delay overridden by env");
	}

	if cfg.twitch.client_id.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false) {
		info!("twitch config: client_id provided by server config");
	} else {
		warn!("twitch config: no client_id in server config (waiting for user OAuth)");
	}

	if let (Some(min), Some(max)) = (cfg.twitch.reconnect_min_delay, cfg.twitch.reconnect_max_delay)
		&& min > max
	{
		warn!(
			min_ms = min.as_millis(),
			max_ms = max.as_millis(),
			"twitch config: reconnect_min_delay > reconnect_max_delay; swapping"
		);
		cfg.twitch.reconnect_min_delay = Some(max);
		cfg.twitch.reconnect_max_delay = Some(min);
	}
}
