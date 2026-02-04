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

	fn auth_header_optional(&self) -> Option<String> {
		let token = self.access_token.trim();
		if token.is_empty() {
			None
		} else {
			Some(format!("Bearer {}", token))
		}
	}

	pub async fn send_chat_message(
		&self,
		broadcaster_user_id: u64,
		content: &str,
		reply_to_message_id: Option<&str>,
	) -> anyhow::Result<()> {
		let url = format!("{}/public/v1/chat", self.base_url.trim_end_matches('/'));
		let body = KickPostChatRequest {
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

		let mut request = self.client.get(url.clone());
		let mut used_auth = false;
		if let Some(auth) = self.auth_header_optional() {
			request = request.header("Authorization", auth);
			used_auth = true;
		}

		let mut resp = request.send().await.context("kick get channels")?;
		if used_auth && matches!(resp.status(), StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
			resp = self.client.get(url).send().await.context("kick get channels (unauth)")?;
		}

		if resp.status() == StatusCode::NOT_FOUND {
			return Ok(None);
		}

		if !resp.status().is_success() {
			return self.resolve_broadcaster_id_v2(slug).await;
		}

		let body: KickChannelsResponse = resp.json().await.context("parse kick channels response")?;
		let found = body
			.data
			.into_iter()
			.find(|c| c.slug.eq_ignore_ascii_case(slug))
			.map(|c| c.broadcaster_user_id);
		if found.is_some() {
			return Ok(found);
		}

		self.resolve_broadcaster_id_v2(slug).await
	}

	pub async fn resolve_chatroom_id(&self, slug: &str) -> anyhow::Result<Option<u64>> {
		let url = format!("https://kick.com/api/v2/channels/{}/chatroom", urlencoding::encode(slug));
		let resp = self
			.client
			.get(url)
			.header("Accept", "application/json")
			.header("User-Agent", "chatty-server/0.1")
			.send()
			.await
			.context("kick get chatroom")?;

		if resp.status() == StatusCode::NOT_FOUND {
			return Ok(None);
		}
		if !resp.status().is_success() {
			return Err(anyhow!("kick get chatroom failed: status={}", resp.status()));
		}

		let body: KickChatroomResponse = resp.json().await.context("parse kick chatroom response")?;
		Ok(Some(body.id))
	}

	async fn resolve_broadcaster_id_v2(&self, slug: &str) -> anyhow::Result<Option<u64>> {
		let url = format!("https://kick.com/api/v2/channels/{}", urlencoding::encode(slug));
		let resp = self
			.client
			.get(url)
			.header("Accept", "application/json")
			.header("User-Agent", "chatty-server/0.1")
			.send()
			.await
			.context("kick get channels (v2)")?;

		if resp.status() == StatusCode::NOT_FOUND {
			return Ok(None);
		}
		if !resp.status().is_success() {
			return Err(anyhow!("kick get channels failed: status={}", resp.status()));
		}

		let body: KickChannelV2Response = resp.json().await.context("parse kick channels response (v2)")?;
		Ok(Some(body.user_id.unwrap_or(body.id)))
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

#[derive(Debug, serde::Serialize)]
struct KickPostChatRequest {
	broadcaster_user_id: u64,
	content: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
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
struct KickChannelV2Response {
	id: u64,
	#[serde(default)]
	user_id: Option<u64>,
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
struct KickChatroomResponse {
	id: u64,
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
