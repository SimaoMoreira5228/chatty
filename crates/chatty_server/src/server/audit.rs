#![forbid(unsafe_code)]

use anyhow::{Context, anyhow};

#[derive(Clone)]
pub struct AuditService {
	backend: Option<AuditBackend>,
}

#[derive(Clone)]
enum AuditBackend {
	Sqlite(sqlx::SqlitePool),
	Postgres(sqlx::PgPool),
	Mysql(sqlx::MySqlPool),
}

impl AuditService {
	pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
		if database_url.starts_with("sqlite:") {
			let pool = sqlx::SqlitePool::connect(database_url).await.context("connect sqlite")?;
			Ok(Self {
				backend: Some(AuditBackend::Sqlite(pool)),
			})
		} else if database_url.starts_with("postgres:") || database_url.starts_with("postgresql:") {
			let pool = sqlx::PgPool::connect(database_url).await.context("connect postgres")?;
			Ok(Self {
				backend: Some(AuditBackend::Postgres(pool)),
			})
		} else if database_url.starts_with("mysql:") || database_url.starts_with("mariadb:") {
			let pool = sqlx::MySqlPool::connect(database_url).await.context("connect mysql")?;
			Ok(Self {
				backend: Some(AuditBackend::Mysql(pool)),
			})
		} else {
			Err(anyhow!("unsupported database_url for audit"))
		}
	}

	pub fn disabled() -> Self {
		Self { backend: None }
	}

	pub async fn record_command(
		&self,
		client_id: &str,
		topic: &str,
		command_kind: &str,
		target_user_id: Option<&str>,
		target_message_id: Option<&str>,
	) -> anyhow::Result<()> {
		let Some(backend) = &self.backend else {
			return Ok(());
		};

		match backend {
			AuditBackend::Sqlite(pool) => {
				sqlx::query(
					"INSERT INTO command_audit (client_id, topic, command_kind, target_user_id, target_message_id, created_at) \
					VALUES (?, ?, ?, ?, ?, strftime('%s','now'))",
				)
				.bind(client_id)
				.bind(topic)
				.bind(command_kind)
				.bind(target_user_id)
				.bind(target_message_id)
				.execute(pool)
				.await
				.context("insert command_audit (sqlite)")?;
			}
			AuditBackend::Postgres(pool) => {
				sqlx::query(
					"INSERT INTO command_audit (client_id, topic, command_kind, target_user_id, target_message_id, created_at) \
					VALUES ($1, $2, $3, $4, $5, NOW())",
				)
				.bind(client_id)
				.bind(topic)
				.bind(command_kind)
				.bind(target_user_id)
				.bind(target_message_id)
				.execute(pool)
				.await
				.context("insert command_audit (postgres)")?;
			}
			AuditBackend::Mysql(pool) => {
				sqlx::query(
					"INSERT INTO command_audit (client_id, topic, command_kind, target_user_id, target_message_id, created_at) \
					VALUES (?, ?, ?, ?, ?, NOW())",
				)
				.bind(client_id)
				.bind(topic)
				.bind(command_kind)
				.bind(target_user_id)
				.bind(target_message_id)
				.execute(pool)
				.await
				.context("insert command_audit (mysql)")?;
			}
		}

		Ok(())
	}
}
