#![forbid(unsafe_code)]

mod adapter;
mod client;

pub use adapter::{KickConfig, KickEventAdapter};
use anyhow::{Context as _, anyhow};
pub use client::{KickClient, KickTokenIntrospection, KickUserInfo};
use reqwest::Client as HttpClient;
use serde::Deserialize;

const KICK_TOKEN_URL: &str = "https://id.kick.com/oauth/token";

pub struct KickValidatedToken {
	pub token: KickTokenIntrospection,
	pub user: Option<KickUserInfo>,
}

pub async fn validate_user_token(token: &str) -> anyhow::Result<KickValidatedToken> {
	let client = KickClient::new("https://api.kick.com", token);
	let introspection = client.introspect_token(token).await?;
	if introspection.active != Some(true) {
		return Err(anyhow!("kick token is not active"));
	}
	let user = client.get_current_user(token).await.ok().flatten();
	Ok(KickValidatedToken {
		token: introspection,
		user,
	})
}

#[derive(Debug, Deserialize)]
pub struct KickTokenRefreshResponse {
	pub access_token: String,
	#[serde(default)]
	pub refresh_token: Option<String>,
	pub expires_in: u64,
}

pub async fn refresh_user_token(
	client_id: &str,
	client_secret: &str,
	refresh_token: &str,
) -> anyhow::Result<KickTokenRefreshResponse> {
	let http = HttpClient::builder()
		.user_agent("chatty/0.x (kick-oauth-refresh)")
		.build()
		.context("build kick refresh client")?;

	let resp = http
		.post(KICK_TOKEN_URL)
		.form(&[
			("grant_type", "refresh_token"),
			("client_id", client_id),
			("client_secret", client_secret),
			("refresh_token", refresh_token),
		])
		.send()
		.await
		.context("kick refresh token request")?;

	let status = resp.status();
	let body = resp.text().await.context("kick refresh token read body")?;

	if !status.is_success() {
		anyhow::bail!("kick refresh token failed: status={status} body={body}");
	}

	serde_json::from_str(&body).context("kick refresh token parse json")
}
