#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};

use chatty_protocol::pb;

/// Shared server state.
#[derive(Debug, Default)]
pub struct GlobalState {
	subs_by_conn: HashMap<u64, HashSet<String>>,

	topic_refcounts: HashMap<String, u64>,
}

impl GlobalState {
	/// Returns a snapshot of subscribed topics for the given connection id.
	pub fn topics_for_conn(&self, conn_id: u64) -> HashSet<String> {
		self.subs_by_conn.get(&conn_id).cloned().unwrap_or_default()
	}

	/// Returns the current global subscriptions snapshot `(topic -> refcount)`.
	pub fn topic_refcounts_snapshot(&self) -> HashMap<String, u64> {
		self.topic_refcounts.clone()
	}

	/// Removes state for a connection and decrements refcounts.
	pub fn remove_conn(&mut self, conn_id: u64) -> Vec<String> {
		let Some(prev) = self.subs_by_conn.remove(&conn_id) else {
			return Vec::new();
		};

		let mut topics_to_leave = Vec::new();

		for topic in prev {
			if !validate_topic(&topic) {
				continue;
			}

			match self.topic_refcounts.get_mut(&topic) {
				Some(rc) => {
					if *rc <= 1 {
						self.topic_refcounts.remove(&topic);
						topics_to_leave.push(topic);
					} else {
						*rc -= 1;
					}
				}
				None => {}
			}
		}

		topics_to_leave
	}

	/// Applies a `Subscribe` request and returns results and join topics.
	pub fn handle_subscribe(&mut self, conn_id: u64, sub: pb::Subscribe) -> (Vec<pb::SubscriptionResult>, Vec<String>) {
		let mut results = Vec::with_capacity(sub.subs.len());
		let mut topics_to_join = Vec::new();

		let topic_set = self.subs_by_conn.entry(conn_id).or_default();

		for s in sub.subs {
			let topic = s.topic;

			let (status, detail) = if validate_topic(&topic) {
				if topic_set.insert(topic.clone()) {
					let rc = self.topic_refcounts.entry(topic.clone()).or_insert(0);
					*rc += 1;

					if *rc == 1 {
						topics_to_join.push(topic.clone());
					}
				}

				(pb::subscription_result::Status::Ok, String::new())
			} else {
				(
					pb::subscription_result::Status::InvalidTopic,
					"expected topic starting with \"room:\"".to_string(),
				)
			};

			results.push(pb::SubscriptionResult {
				topic,
				status: status as i32,
				current_cursor: 0,
				detail,
			});
		}

		(results, topics_to_join)
	}

	/// Applies an `Unsubscribe` request and returns results and leave topics.
	pub fn handle_unsubscribe(&mut self, conn_id: u64, unsub: pb::Unsubscribe) -> (Vec<pb::UnsubscribeResult>, Vec<String>) {
		let mut results = Vec::with_capacity(unsub.topics.len());
		let mut topics_to_leave = Vec::new();

		let mut topic_set_opt = self.subs_by_conn.get_mut(&conn_id);

		for topic in unsub.topics {
			let (status, detail) = match topic_set_opt.as_deref_mut() {
				Some(set) => {
					if !validate_topic(&topic) {
						(
							pb::unsubscribe_result::Status::InvalidTopic,
							"expected topic starting with \"room:\"".to_string(),
						)
					} else if set.remove(&topic) {
						if let Some(rc) = self.topic_refcounts.get_mut(&topic) {
							if *rc <= 1 {
								self.topic_refcounts.remove(&topic);
								topics_to_leave.push(topic.clone());
							} else {
								*rc -= 1;
							}
						}
						(pb::unsubscribe_result::Status::Ok, String::new())
					} else {
						(pb::unsubscribe_result::Status::NotSubscribed, String::new())
					}
				}
				None => (pb::unsubscribe_result::Status::NotSubscribed, String::new()),
			};

			results.push(pb::UnsubscribeResult {
				topic,
				status: status as i32,
				detail,
			});
		}

		(results, topics_to_leave)
	}
}

/// v1 topic validation.
#[inline]
pub fn validate_topic(topic: &str) -> bool {
	topic.starts_with("room:")
}
