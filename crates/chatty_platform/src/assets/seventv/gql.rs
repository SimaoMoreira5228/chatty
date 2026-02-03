#![forbid(unsafe_code)]

use std::sync::OnceLock;

use anyhow::{Context, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::types::{SevenTvPlatform, SevenTvUserEmoteSets};

const SEVENTV_GQL_URL: &str = "https://api.7tv.app/v4/gql";

static SEVENTV_HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn http_client() -> &'static Client {
	SEVENTV_HTTP_CLIENT.get_or_init(Client::new)
}

pub struct SevenTvGqlClient {
	http: &'static Client,
}

impl SevenTvGqlClient {
	pub fn new() -> Self {
		Self { http: http_client() }
	}

	pub async fn user_emote_sets(
		&self,
		platform: SevenTvPlatform,
		platform_id: &str,
	) -> anyhow::Result<SevenTvUserEmoteSets> {
		let req = GraphqlRequest {
			query: USER_EMOTE_SETS_QUERY,
			variables: UserEmoteSetsVars {
				platform: platform.as_str().to_string(),
				platform_id: platform_id.to_string(),
			},
		};

		let data: UserEmoteSetsData = self.post(req, "7tv user emote sets").await?;
		let user = data
			.users
			.and_then(|u| u.user_by_connection)
			.ok_or_else(|| anyhow!("7tv user not found"))?;

		Ok(SevenTvUserEmoteSets {
			active_emote_set_id: user.style.and_then(|s| s.active_emote_set).map(|set| set.id),
			personal_emote_set_id: user.personal_emote_set.map(|set| set.id),
		})
	}

	pub async fn emote_set(&self, emote_set_id: &str) -> anyhow::Result<SevenTvEmoteSet> {
		let req = GraphqlRequest {
			query: EMOTE_SET_QUERY,
			variables: EmoteSetVars {
				emote_set_id: emote_set_id.to_string(),
			},
		};

		let data: EmoteSetData = self.post(req, "7tv emote set").await?;
		let emote_set = data
			.emote_sets
			.and_then(|e| e.emote_set)
			.ok_or_else(|| anyhow!("7tv emote set not found"))?;

		Ok(emote_set)
	}

	pub async fn global_badges(&self) -> anyhow::Result<Vec<SevenTvBadge>> {
		let req = GraphqlRequest {
			query: BADGES_QUERY,
			variables: EmptyVars,
		};

		let data: BadgesData = self.post(req, "7tv global badges").await?;
		Ok(data.badges.map(|b| b.badges).unwrap_or_default())
	}

	pub async fn channel_badges(&self, platform: SevenTvPlatform, platform_id: &str) -> anyhow::Result<SevenTvUserBadges> {
		let req = GraphqlRequest {
			query: CHANNEL_BADGES_QUERY,
			variables: UserEmoteSetsVars {
				platform: platform.as_str().to_string(),
				platform_id: platform_id.to_string(),
			},
		};

		let data: ChannelBadgesData = self.post(req, "7tv channel badges").await?;
		let user = data
			.users
			.and_then(|u| u.user_by_connection)
			.ok_or_else(|| anyhow!("7tv user not found"))?;

		let mut inventory_badges = Vec::new();
		if let Some(inventory) = user.inventory {
			for edge in inventory.badges {
				if let Some(node) = edge.to
					&& let Some(badge) = node.badge
				{
					inventory_badges.push(badge);
				}
			}
		}

		Ok(SevenTvUserBadges {
			active_badge: user.style.and_then(|s| s.active_badge),
			inventory_badges,
		})
	}

	async fn post<T, V>(&self, req: GraphqlRequest<'_, V>, context: &str) -> anyhow::Result<T>
	where
		T: for<'de> Deserialize<'de>,
		V: Serialize,
	{
		let resp = self
			.http
			.post(SEVENTV_GQL_URL)
			.json(&req)
			.send()
			.await
			.with_context(|| format!("{context} gql request"))?
			.error_for_status()
			.with_context(|| format!("{context} gql status"))?;

		let body: GraphqlResponse<T> = resp.json().await.with_context(|| format!("{context} gql json"))?;

		if let Some(errors) = body.errors {
			return Err(anyhow!(
				"{} gql errors: {}",
				context,
				errors.into_iter().map(|e| e.message).collect::<Vec<_>>().join(", ")
			));
		}

		body.data.ok_or_else(|| anyhow!("{context} gql response missing data"))
	}
}

#[derive(Debug, Serialize)]
struct GraphqlRequest<'a, V> {
	query: &'a str,
	variables: V,
}

#[derive(Debug, Deserialize)]
struct GraphqlResponse<T> {
	data: Option<T>,
	errors: Option<Vec<GraphqlError>>,
}

#[derive(Debug, Deserialize)]
struct GraphqlError {
	message: String,
}

#[derive(Debug, Serialize)]
struct UserEmoteSetsVars {
	platform: String,
	#[serde(rename = "platformId")]
	platform_id: String,
}

#[derive(Debug, Serialize)]
struct EmoteSetVars {
	#[serde(rename = "id")]
	emote_set_id: String,
}

#[derive(Debug, Serialize)]
struct EmptyVars;

#[derive(Debug, Deserialize)]
struct UserEmoteSetsData {
	users: Option<UserEmoteSetsUsers>,
}

#[derive(Debug, Deserialize)]
struct UserEmoteSetsUsers {
	#[serde(rename = "userByConnection")]
	user_by_connection: Option<UserEmoteSetsUser>,
}

#[derive(Debug, Deserialize)]
struct UserEmoteSetsUser {
	#[serde(rename = "personalEmoteSet")]
	personal_emote_set: Option<SevenTvEmoteSetId>,
	#[serde(default)]
	style: Option<UserStyleEmoteSet>,
}

#[derive(Debug, Deserialize)]
struct UserStyleEmoteSet {
	#[serde(rename = "activeEmoteSet")]
	active_emote_set: Option<SevenTvEmoteSetId>,
}

#[derive(Debug, Deserialize)]
struct SevenTvEmoteSetId {
	id: String,
}

#[derive(Debug, Deserialize)]
struct EmoteSetData {
	#[serde(rename = "emoteSets")]
	emote_sets: Option<EmoteSetQuery>,
}

#[derive(Debug, Deserialize)]
struct EmoteSetQuery {
	#[serde(rename = "emoteSet")]
	emote_set: Option<SevenTvEmoteSet>,
}

#[derive(Debug, Deserialize)]
struct BadgesData {
	badges: Option<BadgesQuery>,
}

#[derive(Debug, Deserialize)]
struct BadgesQuery {
	#[serde(default)]
	badges: Vec<SevenTvBadge>,
}

#[derive(Debug, Deserialize)]
struct ChannelBadgesData {
	users: Option<ChannelBadgesUsers>,
}

#[derive(Debug, Deserialize)]
struct ChannelBadgesUsers {
	#[serde(rename = "userByConnection")]
	user_by_connection: Option<ChannelBadgesUser>,
}

#[derive(Debug, Deserialize)]
struct ChannelBadgesUser {
	#[serde(default)]
	inventory: Option<SevenTvInventory>,
	#[serde(default)]
	style: Option<UserStyleBadge>,
}

#[derive(Debug, Deserialize)]
struct UserStyleBadge {
	#[serde(rename = "activeBadge")]
	active_badge: Option<SevenTvBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvInventory {
	#[serde(default)]
	badges: Vec<SevenTvEntitlementEdgeBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvEntitlementEdgeBadge {
	#[serde(default)]
	to: Option<SevenTvEntitlementNodeBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvEntitlementNodeBadge {
	#[serde(default)]
	badge: Option<SevenTvBadge>,
}

#[derive(Debug, Deserialize)]
pub struct SevenTvEmoteSet {
	pub id: String,
	pub emotes: SevenTvEmoteSetEmoteSearch,
}

#[derive(Debug, Deserialize)]
pub struct SevenTvEmoteSetEmoteSearch {
	pub items: Vec<SevenTvEmoteSetEmote>,
}

#[derive(Debug, Deserialize)]
pub struct SevenTvEmoteSetEmote {
	pub alias: String,
	pub emote: SevenTvEmote,
}

#[derive(Debug, Deserialize)]
pub struct SevenTvEmote {
	pub id: String,
	#[serde(rename = "defaultName")]
	pub default_name: String,
	#[serde(default)]
	pub images: Vec<SevenTvImage>,
}

#[derive(Debug, Deserialize)]
pub struct SevenTvImage {
	pub url: String,
	pub width: i32,
	pub height: i32,
	pub mime: String,
	pub scale: i32,
}

#[derive(Debug, Deserialize)]
pub struct SevenTvBadge {
	pub id: String,
	pub name: String,
	#[serde(default)]
	pub images: Vec<SevenTvImage>,
}

#[derive(Debug)]
pub struct SevenTvUserBadges {
	pub active_badge: Option<SevenTvBadge>,
	pub inventory_badges: Vec<SevenTvBadge>,
}

const USER_EMOTE_SETS_QUERY: &str = r#"
query UserEmoteSets($platform: Platform!, $platformId: String!) {
  users {
		userByConnection(platform: $platform, platformId: $platformId) {
      personalEmoteSet {
        id
      }
      style {
        activeEmoteSet {
          id
        }
      }
    }
  }
}
"#;

const EMOTE_SET_QUERY: &str = r#"
query EmoteSet($id: Id!) {
  emoteSets {
    emoteSet(id: $id) {
      id
      emotes {
        items {
          alias
          emote {
            id
            defaultName
            images {
              url
              width
              height
              mime
              scale
            }
          }
        }
      }
    }
  }
}
"#;

const BADGES_QUERY: &str = r#"
query Badges {
  badges {
    badges {
      id
      name
      images {
        url
        width
        height
        mime
        scale
      }
    }
  }
}
"#;

const CHANNEL_BADGES_QUERY: &str = r#"
query ChannelBadges($platform: Platform!, $platformId: String!) {
  users {
    userByConnection(platform: $platform, platformId: $platformId) {
      id
      style {
        activeBadge {
          id
          name
          images {
            url
            width
            height
            mime
            scale
          }
        }
      }
      inventory {
        badges {
          to {
            badge {
              id
              name
              images {
                url
                width
                height
                mime
                scale
              }
            }
          }
        }
      }
    }
  }
}
"#;
