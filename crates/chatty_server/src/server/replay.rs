#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use anyhow::{Context, anyhow};
use chatty_protocol::pb;
use prost::Message;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ReplayStoreConfig {
	pub per_topic_capacity: usize,
	pub retention_secs: Option<u64>,
}

impl Default for ReplayStoreConfig {
	fn default() -> Self {
		Self {
			per_topic_capacity: 2048,
			retention_secs: None,
		}
	}
}

#[derive(Debug, Clone)]
pub struct ReplayOutcome {
	pub status: pb::subscription_result::Status,
	pub current_cursor: u64,
	pub items: Vec<pb::EventEnvelope>,
}

#[derive(Debug, Default)]
pub struct ReplayStore {
	clients: HashMap<String, ClientReplay>,
}

#[derive(Debug, Default)]
struct ClientReplay {
	current_cursor_by_topic: HashMap<String, u64>,
	buffer_by_topic: HashMap<String, VecDeque<pb::EventEnvelope>>,
}

impl ReplayStore {
	pub fn push_event(
		&mut self,
		client_id: &str,
		topic: &str,
		mut env: pb::EventEnvelope,
		cfg: &ReplayStoreConfig,
	) -> pb::EventEnvelope {
		let client = self.clients.entry(client_id.to_string()).or_default();
		let cursor = client.current_cursor_by_topic.entry(topic.to_string()).or_insert(0);
		*cursor = cursor.saturating_add(1);
		env.cursor = *cursor;
		let buf = client.buffer_by_topic.entry(topic.to_string()).or_default();
		buf.push_back(env.clone());

		if let Some(retention) = cfg.retention_secs {
			let now_ms = if env.server_time_unix_ms > 0 {
				env.server_time_unix_ms
			} else {
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.unwrap_or_default()
					.as_millis() as i64
			};
			let threshold_ms = now_ms.saturating_sub((retention as i64).saturating_mul(1000));
			while let Some(front) = buf.front() {
				if front.server_time_unix_ms == 0 {
					break;
				}
				if front.server_time_unix_ms < threshold_ms {
					buf.pop_front();
				} else {
					break;
				}
			}
		}

		while buf.len() > cfg.per_topic_capacity {
			buf.pop_front();
		}
		env
	}

	pub fn replay(&self, client_id: &str, topic: &str, last_cursor: u64) -> ReplayOutcome {
		let client = match self.clients.get(client_id) {
			Some(c) => c,
			None => {
				return ReplayOutcome {
					status: pb::subscription_result::Status::Ok,
					current_cursor: 0,
					items: Vec::new(),
				};
			}
		};

		let current_cursor = client.current_cursor_by_topic.get(topic).copied().unwrap_or(0);
		let Some(buf) = client.buffer_by_topic.get(topic) else {
			let status = if last_cursor == 0 {
				pb::subscription_result::Status::Ok
			} else {
				pb::subscription_result::Status::ReplayNotAvailable
			};
			return ReplayOutcome {
				status,
				current_cursor,
				items: Vec::new(),
			};
		};

		let oldest_cursor = buf.front().map(|e| e.cursor).unwrap_or(0);
		if last_cursor > 0 && last_cursor < oldest_cursor {
			return ReplayOutcome {
				status: pb::subscription_result::Status::ReplayNotAvailable,
				current_cursor,
				items: Vec::new(),
			};
		}

		let items = buf.iter().filter(|e| e.cursor > last_cursor).cloned().collect::<Vec<_>>();
		ReplayOutcome {
			status: pb::subscription_result::Status::Ok,
			current_cursor,
			items,
		}
	}
}

#[async_trait::async_trait]
pub trait ReplayBackend: Send + Sync {
	async fn push_event(
		&self,
		client_id: &str,
		topic: &str,
		env: pb::EventEnvelope,
		cfg: &ReplayStoreConfig,
	) -> anyhow::Result<pb::EventEnvelope>;

	async fn replay(&self, client_id: &str, topic: &str, last_cursor: u64) -> anyhow::Result<ReplayOutcome>;
}

pub struct InMemoryReplayBackend {
	inner: Mutex<ReplayStore>,
}

impl Default for InMemoryReplayBackend {
	fn default() -> Self {
		Self {
			inner: Mutex::new(ReplayStore::default()),
		}
	}
}

#[async_trait::async_trait]
impl ReplayBackend for InMemoryReplayBackend {
	async fn push_event(
		&self,
		client_id: &str,
		topic: &str,
		env: pb::EventEnvelope,
		cfg: &ReplayStoreConfig,
	) -> anyhow::Result<pb::EventEnvelope> {
		let mut guard = self.inner.lock().await;
		Ok(guard.push_event(client_id, topic, env, cfg))
	}

	async fn replay(&self, client_id: &str, topic: &str, last_cursor: u64) -> anyhow::Result<ReplayOutcome> {
		let guard = self.inner.lock().await;
		Ok(guard.replay(client_id, topic, last_cursor))
	}
}

#[derive(Clone)]
pub struct PersistentReplayBackend {
	backend: PersistentBackend,
	per_topic_capacity: usize,
}

#[derive(Clone)]
enum PersistentBackend {
	Sqlite(sqlx::SqlitePool),
	Postgres(sqlx::PgPool),
	Mysql(sqlx::MySqlPool),
}

impl PersistentReplayBackend {
	pub async fn connect(database_url: &str, per_topic_capacity: usize) -> anyhow::Result<Self> {
		if database_url.starts_with("sqlite:") {
			let pool = sqlx::SqlitePool::connect(database_url).await.context("connect sqlite")?;
			sqlx::migrate!("migrations/sqlite")
				.run(&pool)
				.await
				.context("run sqlite migrations")?;

			Ok(Self {
				backend: PersistentBackend::Sqlite(pool),
				per_topic_capacity,
			})
		} else if database_url.starts_with("postgres:") || database_url.starts_with("postgresql:") {
			let pool = sqlx::PgPool::connect(database_url).await.context("connect postgres")?;
			sqlx::migrate!("migrations/postgres")
				.run(&pool)
				.await
				.context("run postgres migrations")?;

			Ok(Self {
				backend: PersistentBackend::Postgres(pool),
				per_topic_capacity,
			})
		} else if database_url.starts_with("mysql:") || database_url.starts_with("mariadb:") {
			let pool = sqlx::MySqlPool::connect(database_url).await.context("connect mysql")?;
			sqlx::migrate!("migrations/mysql")
				.run(&pool)
				.await
				.context("run mysql migrations")?;

			Ok(Self {
				backend: PersistentBackend::Mysql(pool),
				per_topic_capacity,
			})
		} else {
			Err(anyhow!("unsupported database_url (use sqlite:, postgres:, mysql:)"))
		}
	}

	async fn next_cursor(&self, client_id: &str, topic: &str) -> anyhow::Result<u64> {
		match &self.backend {
			PersistentBackend::Sqlite(pool) => {
				let mut tx = pool.begin().await.context("begin sqlite tx")?;
				let row: Option<(i64,)> =
					sqlx::query_as("SELECT cursor FROM replay_cursors WHERE client_id = ? AND topic = ?")
						.bind(client_id)
						.bind(topic)
						.fetch_optional(&mut *tx)
						.await
						.context("select cursor (sqlite)")?;

				let next = row.map(|(c,)| c as u64 + 1).unwrap_or(1);
				sqlx::query(
					"INSERT INTO replay_cursors (client_id, topic, cursor) VALUES (?, ?, ?) \
					ON CONFLICT(client_id, topic) DO UPDATE SET cursor = excluded.cursor",
				)
				.bind(client_id)
				.bind(topic)
				.bind(next as i64)
				.execute(&mut *tx)
				.await
				.context("upsert cursor (sqlite)")?;

				tx.commit().await.context("commit sqlite tx")?;
				Ok(next)
			}
			PersistentBackend::Postgres(pool) => {
				let mut tx = pool.begin().await.context("begin postgres tx")?;

				let row: Option<(i64,)> =
					sqlx::query_as("SELECT cursor FROM replay_cursors WHERE client_id = $1 AND topic = $2 FOR UPDATE")
						.bind(client_id)
						.bind(topic)
						.fetch_optional(&mut *tx)
						.await
						.context("select cursor (postgres)")?;

				let next = row.map(|(c,)| c as u64 + 1).unwrap_or(1);
				sqlx::query(
					"INSERT INTO replay_cursors (client_id, topic, cursor) VALUES ($1, $2, $3) \
					ON CONFLICT (client_id, topic) DO UPDATE SET cursor = EXCLUDED.cursor",
				)
				.bind(client_id)
				.bind(topic)
				.bind(next as i64)
				.execute(&mut *tx)
				.await
				.context("upsert cursor (postgres)")?;

				tx.commit().await.context("commit postgres tx")?;
				Ok(next)
			}
			PersistentBackend::Mysql(pool) => {
				let mut tx = pool.begin().await.context("begin mysql tx")?;
				let row: Option<(i64,)> =
					sqlx::query_as("SELECT cursor FROM replay_cursors WHERE client_id = ? AND topic = ? FOR UPDATE")
						.bind(client_id)
						.bind(topic)
						.fetch_optional(&mut *tx)
						.await
						.context("select cursor (mysql)")?;

				let next = row.map(|(c,)| c as u64 + 1).unwrap_or(1);
				sqlx::query(
					"INSERT INTO replay_cursors (client_id, topic, cursor) VALUES (?, ?, ?) \
					ON DUPLICATE KEY UPDATE cursor = VALUES(cursor)",
				)
				.bind(client_id)
				.bind(topic)
				.bind(next as i64)
				.execute(&mut *tx)
				.await
				.context("upsert cursor (mysql)")?;

				tx.commit().await.context("commit mysql tx")?;
				Ok(next)
			}
		}
	}
}

#[async_trait::async_trait]
impl ReplayBackend for PersistentReplayBackend {
	async fn push_event(
		&self,
		client_id: &str,
		topic: &str,
		mut env: pb::EventEnvelope,
		cfg: &ReplayStoreConfig,
	) -> anyhow::Result<pb::EventEnvelope> {
		let cursor = self.next_cursor(client_id, topic).await?;
		env.cursor = cursor;
		let payload = prost::Message::encode_to_vec(&env);
		let cap = cfg.per_topic_capacity;
		let cap = if cap == 0 { self.per_topic_capacity } else { cap };
		let retention_secs = cfg.retention_secs;

		match &self.backend {
			PersistentBackend::Sqlite(pool) => {
				sqlx::query(
					"INSERT INTO replay_events (client_id, topic, cursor, payload, created_at) VALUES (?, ?, ?, ?, strftime('%s','now'))",
				)
				.bind(client_id)
				.bind(topic)
				.bind(cursor as i64)
				.bind(payload)
				.execute(pool)
				.await
				.context("insert replay event (sqlite)")?;

				if cap > 0 {
					let threshold = cursor.saturating_sub(cap as u64) as i64;
					sqlx::query("DELETE FROM replay_events WHERE client_id = ? AND topic = ? AND cursor <= ?")
						.bind(client_id)
						.bind(topic)
						.bind(threshold)
						.execute(pool)
						.await
						.context("trim replay events (sqlite)")?;
				}

				if let Some(retention) = retention_secs {
					let now = std::time::SystemTime::now()
						.duration_since(std::time::UNIX_EPOCH)
						.unwrap_or_default()
						.as_secs() as i64;
					let threshold = now.saturating_sub(retention as i64);
					sqlx::query("DELETE FROM replay_events WHERE client_id = ? AND topic = ? AND created_at < ?")
						.bind(client_id)
						.bind(topic)
						.bind(threshold)
						.execute(pool)
						.await
						.context("retention trim replay events (sqlite)")?;
				}
			}
			PersistentBackend::Postgres(pool) => {
				sqlx::query(
					"INSERT INTO replay_events (client_id, topic, cursor, payload, created_at) VALUES ($1, $2, $3, $4, NOW())",
				)
				.bind(client_id)
				.bind(topic)
				.bind(cursor as i64)
				.bind(payload)
				.execute(pool)
				.await
				.context("insert replay event (postgres)")?;

				if cap > 0 {
					let threshold = cursor.saturating_sub(cap as u64) as i64;
					sqlx::query("DELETE FROM replay_events WHERE client_id = $1 AND topic = $2 AND cursor <= $3")
						.bind(client_id)
						.bind(topic)
						.bind(threshold)
						.execute(pool)
						.await
						.context("trim replay events (postgres)")?;
				}

				if let Some(retention) = retention_secs {
					let now = std::time::SystemTime::now()
						.duration_since(std::time::UNIX_EPOCH)
						.unwrap_or_default()
						.as_secs() as i64;
					let threshold = now.saturating_sub(retention as i64);
					sqlx::query(
						"DELETE FROM replay_events WHERE client_id = $1 AND topic = $2 AND created_at < to_timestamp($3)",
					)
					.bind(client_id)
					.bind(topic)
					.bind(threshold)
					.execute(pool)
					.await
					.context("retention trim replay events (postgres)")?;
				}
			}
			PersistentBackend::Mysql(pool) => {
				sqlx::query(
					"INSERT INTO replay_events (client_id, topic, cursor, payload, created_at) VALUES (?, ?, ?, ?, NOW())",
				)
				.bind(client_id)
				.bind(topic)
				.bind(cursor as i64)
				.bind(payload)
				.execute(pool)
				.await
				.context("insert replay event (mysql)")?;

				if cap > 0 {
					let threshold = cursor.saturating_sub(cap as u64) as i64;
					sqlx::query("DELETE FROM replay_events WHERE client_id = ? AND topic = ? AND cursor <= ?")
						.bind(client_id)
						.bind(topic)
						.bind(threshold)
						.execute(pool)
						.await
						.context("trim replay events (mysql)")?;
				}

				if let Some(retention) = retention_secs {
					let now = std::time::SystemTime::now()
						.duration_since(std::time::UNIX_EPOCH)
						.unwrap_or_default()
						.as_secs() as i64;
					let threshold = now.saturating_sub(retention as i64);
					sqlx::query(
						"DELETE FROM replay_events WHERE client_id = ? AND topic = ? AND created_at < FROM_UNIXTIME(?)",
					)
					.bind(client_id)
					.bind(topic)
					.bind(threshold)
					.execute(pool)
					.await
					.context("retention trim replay events (mysql)")?;
				}
			}
		}

		Ok(env)
	}

	async fn replay(&self, client_id: &str, topic: &str, last_cursor: u64) -> anyhow::Result<ReplayOutcome> {
		match &self.backend {
			PersistentBackend::Sqlite(pool) => {
				let cursor_row: Option<(i64,)> =
					sqlx::query_as("SELECT cursor FROM replay_cursors WHERE client_id = ? AND topic = ?")
						.bind(client_id)
						.bind(topic)
						.fetch_optional(pool)
						.await
						.context("select cursor (sqlite)")?;

				let current_cursor = cursor_row.map(|(c,)| c as u64).unwrap_or(0);
				let rows = sqlx::query_as::<_, (Vec<u8>,)>(
					"SELECT payload FROM replay_events WHERE client_id = ? AND topic = ? AND cursor > ? ORDER BY cursor ASC",
				)
				.bind(client_id)
				.bind(topic)
				.bind(last_cursor as i64)
				.fetch_all(pool)
				.await
				.context("select replay events (sqlite)")?;

				let mut items = Vec::with_capacity(rows.len());
				for (payload,) in rows {
					let env = pb::EventEnvelope::decode(payload.as_slice()).context("decode replay event")?;
					items.push(env);
				}

				let status = if last_cursor > 0 && items.is_empty() && current_cursor > 0 {
					pb::subscription_result::Status::ReplayNotAvailable
				} else {
					pb::subscription_result::Status::Ok
				};

				Ok(ReplayOutcome {
					status,
					current_cursor,
					items,
				})
			}
			PersistentBackend::Postgres(pool) => {
				let cursor_row: Option<(i64,)> =
					sqlx::query_as("SELECT cursor FROM replay_cursors WHERE client_id = $1 AND topic = $2")
						.bind(client_id)
						.bind(topic)
						.fetch_optional(pool)
						.await
						.context("select cursor (postgres)")?;

				let current_cursor = cursor_row.map(|(c,)| c as u64).unwrap_or(0);
				let rows = sqlx::query_as::<_, (Vec<u8>,)>(
					"SELECT payload FROM replay_events WHERE client_id = $1 AND topic = $2 AND cursor > $3 ORDER BY cursor ASC",
				)
				.bind(client_id)
				.bind(topic)
				.bind(last_cursor as i64)
				.fetch_all(pool)
				.await
				.context("select replay events (postgres)")?;

				let mut items = Vec::with_capacity(rows.len());
				for (payload,) in rows {
					let env = pb::EventEnvelope::decode(payload.as_slice()).context("decode replay event")?;
					items.push(env);
				}

				let status = if last_cursor > 0 && items.is_empty() && current_cursor > 0 {
					pb::subscription_result::Status::ReplayNotAvailable
				} else {
					pb::subscription_result::Status::Ok
				};

				Ok(ReplayOutcome {
					status,
					current_cursor,
					items,
				})
			}
			PersistentBackend::Mysql(pool) => {
				let cursor_row: Option<(i64,)> =
					sqlx::query_as("SELECT cursor FROM replay_cursors WHERE client_id = ? AND topic = ?")
						.bind(client_id)
						.bind(topic)
						.fetch_optional(pool)
						.await
						.context("select cursor (mysql)")?;

				let current_cursor = cursor_row.map(|(c,)| c as u64).unwrap_or(0);
				let rows = sqlx::query_as::<_, (Vec<u8>,)>(
					"SELECT payload FROM replay_events WHERE client_id = ? AND topic = ? AND cursor > ? ORDER BY cursor ASC",
				)
				.bind(client_id)
				.bind(topic)
				.bind(last_cursor as i64)
				.fetch_all(pool)
				.await
				.context("select replay events (mysql)")?;

				let mut items = Vec::with_capacity(rows.len());
				for (payload,) in rows {
					let env = pb::EventEnvelope::decode(payload.as_slice()).context("decode replay event")?;
					items.push(env);
				}

				let status = if last_cursor > 0 && items.is_empty() && current_cursor > 0 {
					pb::subscription_result::Status::ReplayNotAvailable
				} else {
					pb::subscription_result::Status::Ok
				};

				Ok(ReplayOutcome {
					status,
					current_cursor,
					items,
				})
			}
		}
	}
}

#[derive(Clone)]
pub struct ReplayService {
	backend: Arc<dyn ReplayBackend>,
	cfg: ReplayStoreConfig,
	enabled: bool,
}

impl ReplayService {
	pub fn new_in_memory(cfg: ReplayStoreConfig) -> Self {
		let per_topic_capacity = cfg.per_topic_capacity;
		let enabled = per_topic_capacity > 0;
		Self {
			backend: Arc::new(InMemoryReplayBackend::default()),
			cfg,
			enabled,
		}
	}

	pub fn new_persistent(backend: PersistentReplayBackend, cfg: ReplayStoreConfig) -> Self {
		let per_topic_capacity = cfg.per_topic_capacity;
		let enabled = per_topic_capacity > 0;
		Self {
			backend: Arc::new(backend),
			cfg,
			enabled,
		}
	}

	pub fn disable_replay() -> Self {
		Self {
			backend: Arc::new(InMemoryReplayBackend::default()),
			cfg: ReplayStoreConfig {
				per_topic_capacity: 0,
				retention_secs: None,
			},
			enabled: false,
		}
	}

	pub async fn push_event(
		&self,
		client_id: &str,
		topic: &str,
		env: pb::EventEnvelope,
	) -> anyhow::Result<pb::EventEnvelope> {
		self.backend.push_event(client_id, topic, env, &self.cfg).await
	}

	pub async fn replay(&self, client_id: &str, topic: &str, last_cursor: u64) -> anyhow::Result<ReplayOutcome> {
		if !self.enabled {
			let status = if last_cursor == 0 {
				pb::subscription_result::Status::Ok
			} else {
				pb::subscription_result::Status::ReplayNotAvailable
			};
			return Ok(ReplayOutcome {
				status,
				current_cursor: 0,
				items: Vec::new(),
			});
		}
		self.backend.replay(client_id, topic, last_cursor).await
	}
}
