#![forbid(unsafe_code)]

use anyhow::{Context, anyhow};
use reqwest::StatusCode;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct KickClient {
	base_url: String,
	access_token: String,
	client: reqwest::Client,
}

impl KickClient {
	pub fn new(base_url: impl Into<String>, access_token: impl Into<String>) -> Self {
		Self {
			base_url: base_url.into(),
			access_token: access_token.into(),
			client: reqwest::Client::new(),
		}
	}

	pub fn set_access_token(&mut self, token: impl Into<String>) {
		self.access_token = token.into();
	}

	fn auth_header(&self) -> anyhow::Result<String> {
		if self.access_token.trim().is_empty() {
			return Err(anyhow!("missing kick access token"));
		}
		Ok(format!("Bearer {}", self.access_token.trim()))
	}

	pub async fn send_chat_message(
		&self,
		broadcaster_user_id: u64,
		content: &str,
		reply_to_message_id: Option<&str>,
	) -> anyhow::Result<()> {
		let url = format!("{}/public/v1/chat", self.base_url.trim_end_matches('/'));
		let body = KickSendChatRequest {
			broadcaster_user_id,
			content: content.to_string(),
			reply_to_message_id: reply_to_message_id.map(|v| v.to_string()),
			type_field: "user".to_string(),
		};

		let resp = self
			.client
			.post(url)
			.header("Authorization", self.auth_header()?)
			.json(&body)
			.send()
			.await
			.context("kick send chat")?;

		match resp.status() {
			StatusCode::OK | StatusCode::CREATED => Ok(()),
			status => Err(anyhow!("kick send chat failed: status={}", status)),
		}
	}

	pub async fn delete_chat_message(&self, message_id: &str) -> anyhow::Result<()> {
		let url = format!("{}/public/v1/chat/{}", self.base_url.trim_end_matches('/'), message_id);
		let resp = self
			.client
			.delete(url)
			.header("Authorization", self.auth_header()?)
			.send()
			.await
			.context("kick delete chat")?;

		match resp.status() {
			StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
			status => Err(anyhow!("kick delete chat failed: status={}", status)),
		}
	}

	pub async fn ban_user(
		&self,
		broadcaster_user_id: u64,
		user_id: u64,
		duration_seconds: Option<u32>,
		reason: Option<&str>,
	) -> anyhow::Result<()> {
		let url = format!("{}/public/v1/moderation/bans", self.base_url.trim_end_matches('/'));
		let body = KickBanRequest {
			broadcaster_user_id,
			user_id,
			duration: duration_seconds,
			reason: reason.map(|v| v.to_string()),
		};
		let resp = self
			.client
			.post(url)
			.header("Authorization", self.auth_header()?)
			.json(&body)
			.send()
			.await
			.context("kick ban user")?;

		match resp.status() {
			StatusCode::OK | StatusCode::CREATED => Ok(()),
			status => Err(anyhow!("kick ban user failed: status={}", status)),
		}
	}

	pub async fn resolve_broadcaster_id(&self, slug: &str) -> anyhow::Result<Option<u64>> {
		let url = format!(
			"{}/public/v1/channels?slug={}",
			self.base_url.trim_end_matches('/'),
			urlencoding::encode(slug)
		);

		let resp = self
			.client
			.get(url)
			.header("Authorization", self.auth_header()?)
			.send()
			.await
			.context("kick get channels")?;

		if resp.status() == StatusCode::NOT_FOUND {
			return Ok(None);
		}
		if !resp.status().is_success() {
			return Err(anyhow!("kick get channels failed: status={}", resp.status()));
		}

		let body: KickChannelsResponse = resp.json().await.context("parse kick channels response")?;
		let found = body
			.data
			.into_iter()
			.find(|c| c.slug.eq_ignore_ascii_case(slug))
			.map(|c| c.broadcaster_user_id);
		Ok(found)
	}

	pub async fn list_event_subscriptions(
		&self,
		broadcaster_user_id: Option<u64>,
	) -> anyhow::Result<Vec<KickEventSubscription>> {
		let mut url = format!("{}/public/v1/events/subscriptions", self.base_url.trim_end_matches('/'));
		if let Some(id) = broadcaster_user_id {
			url.push_str(&format!("?broadcaster_user_id={}", id));
		}

		let resp = self
			.client
			.get(url)
			.header("Authorization", self.auth_header()?)
			.send()
			.await
			.context("kick list event subscriptions")?;

		if !resp.status().is_success() {
			return Err(anyhow!("kick list event subscriptions failed: status={}", resp.status()));
		}

		let body: KickEventsSubscriptionList = resp.json().await.context("parse kick subscriptions list")?;
		Ok(body.data.unwrap_or_default())
	}

	pub async fn create_event_subscriptions(
		&self,
		broadcaster_user_id: Option<u64>,
		events: Vec<KickEventSpec>,
	) -> anyhow::Result<Vec<KickEventSubscription>> {
		let url = format!("{}/public/v1/events/subscriptions", self.base_url.trim_end_matches('/'));
		let body = KickEventsSubscriptionCreate {
			broadcaster_user_id,
			events,
			method: "webhook".to_string(),
		};
		let resp = self
			.client
			.post(url)
			.header("Authorization", self.auth_header()?)
			.json(&body)
			.send()
			.await
			.context("kick create event subscriptions")?;

		if !resp.status().is_success() {
			return Err(anyhow!("kick create event subscriptions failed: status={}", resp.status()));
		}

		let body: KickEventsSubscriptionCreateResponse = resp.json().await.context("parse kick subscriptions create")?;
		Ok(body.data.unwrap_or_default())
	}

	pub async fn introspect_token(&self, token: &str) -> anyhow::Result<KickTokenIntrospection> {
		let url = format!("{}/oauth/token/introspect", self.base_url.trim_end_matches('/'));
		let resp = self
			.client
			.post(url)
			.header("Authorization", format!("Bearer {}", token.trim()))
			.send()
			.await
			.context("kick token introspect")?;

		if !resp.status().is_success() {
			return Err(anyhow!("kick token introspect failed: status={}", resp.status()));
		}

		let body: KickTokenIntrospectionResponse = resp.json().await.context("parse kick token introspect")?;
		Ok(body.data)
	}

	pub async fn fetch_public_key(&self) -> anyhow::Result<String> {
		let url = format!("{}/public/v1/public-key", self.base_url.trim_end_matches('/'));
		let resp = self.client.get(url).send().await.context("kick public key")?;

		if !resp.status().is_success() {
			return Err(anyhow!("kick public key failed: status={}", resp.status()));
		}

		let body: KickPublicKeyResponse = resp.json().await.context("parse kick public key")?;
		Ok(body.data.public_key)
	}

	pub async fn get_current_user(&self, token: &str) -> anyhow::Result<Option<KickUserInfo>> {
		let url = format!("{}/public/v1/users", self.base_url.trim_end_matches('/'));
		let resp = self
			.client
			.get(url)
			.header("Authorization", format!("Bearer {}", token.trim()))
			.send()
			.await
			.context("kick get current user")?;

		if !resp.status().is_success() {
			return Err(anyhow!("kick get current user failed: status={}", resp.status()));
		}

		let body: KickUsersResponse = resp.json().await.context("parse kick users response")?;
		Ok(body.data.and_then(|mut users| users.pop()))
	}
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct KickEventSpec {
	pub name: String,
	pub version: u32,
}

impl KickEventSpec {
	pub fn new(name: impl Into<String>, version: u32) -> Self {
		Self {
			name: name.into(),
			version,
		}
	}
}

#[derive(Debug, serde::Serialize)]
struct KickSendChatRequest {
	broadcaster_user_id: u64,
	content: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	reply_to_message_id: Option<String>,
	#[serde(rename = "type")]
	type_field: String,
}

#[derive(Debug, serde::Serialize)]
struct KickBanRequest {
	broadcaster_user_id: u64,
	user_id: u64,
	#[serde(skip_serializing_if = "Option::is_none")]
	duration: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KickChannelsResponse {
	data: Vec<KickChannelData>,
}

#[derive(Debug, Deserialize)]
struct KickChannelData {
	broadcaster_user_id: u64,
	slug: String,
}

#[derive(Debug, Deserialize)]
struct KickEventsSubscriptionList {
	data: Option<Vec<KickEventSubscription>>,
}

#[derive(Debug, Deserialize)]
struct KickEventsSubscriptionCreateResponse {
	data: Option<Vec<KickEventSubscription>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KickEventSubscription {
	pub id: Option<String>,
	pub event: String,
	pub version: u32,
}

#[derive(Debug, serde::Serialize)]
struct KickEventsSubscriptionCreate {
	#[serde(skip_serializing_if = "Option::is_none")]
	broadcaster_user_id: Option<u64>,
	events: Vec<KickEventSpec>,
	method: String,
}

#[derive(Debug, Deserialize)]
struct KickTokenIntrospectionResponse {
	data: KickTokenIntrospection,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KickTokenIntrospection {
	pub active: Option<bool>,
	pub client_id: Option<String>,
	pub exp: Option<u64>,
	pub scope: Option<String>,
	pub token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KickPublicKeyResponse {
	data: KickPublicKeyData,
}

#[derive(Debug, Deserialize)]
struct KickPublicKeyData {
	public_key: String,
}

#[derive(Debug, Deserialize)]
struct KickUsersResponse {
	data: Option<Vec<KickUserInfo>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KickUserInfo {
	pub user_id: u64,
	pub name: String,
}
