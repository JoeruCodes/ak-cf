use crate::{Env, JsValue};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use worker::*;

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct Notification {
    pub notification_id: String,
    pub user_id: String,
    pub notification_type: NotificationType,
    pub message: String,
    pub timestamp: i64,
    pub read: Read,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub enum NotificationType {
    Referral,
    Performance,
    System,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub enum Read {
    Yes,
    No,
}

impl NotificationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationType::Referral => "Referral",
            NotificationType::Performance => "Performance",
            NotificationType::System => "System",
        }
    }
}

impl Read {
    pub fn as_str(&self) -> &'static str {
        match self {
            Read::Yes => "Yes",
            Read::No => "No",
        }
    }
}

#[derive(Deserialize)]
pub struct TaskResultInput {
    pub player_ranking: Vec<String>,      // e.g. ["addr1", "addr2", ..., "addr5"]
    pub flagged_players: Vec<String>,     // subset of player_ranking
    pub datapoint_id: String,             // for metadata
}

pub async fn push_notification_to_user_do(
    env: &Env,
    user_id: &str,
    notification_type: NotificationType,
    message: &str,
    metadata: Option<HashMap<String, String>>,
) -> Result<()> {
    // 1. Get the DO namespace and stub
    let namespace = env.durable_object("USER_DATA_WRAPPER")?;
    let do_id = namespace.id_from_name(user_id)?;
    let mut stub = do_id.get_stub()?;

    // 2. Build a lightweight Notification and send it to the DO
    let notification = Notification {
        notification_id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        notification_type,
        message: message.to_string(),
        timestamp: Utc::now().timestamp(),
        read: Read::No,
        metadata,
    };

    let request_body = serde_json::json!({
        "user_id": user_id,
        "op": {
            "AddNotificationInternal": notification  // ⬅ You’ll need to add this Op variant
        }
    });

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(JsValue::from_str(&request_body.to_string())));

    let req = Request::new_with_init("https://dummy-url", &init)?;

    stub.fetch_with_request(req).await?;

    Ok(())
}


pub async fn notify_task_result(input: TaskResultInput, env: &Env) -> Result<()> {
    let unflagged_players: Vec<_> = input
        .player_ranking
        .iter()
        .filter(|user_id| !input.flagged_players.contains(user_id))
        .collect();

    for user_id in &input.player_ranking {
        let is_flagged = input.flagged_players.contains(user_id);

        let (message, akai, iq) = if is_flagged {
            ("You did the task wrong.".to_string(), -10, -15)
        } else {
            let rank = unflagged_players
                .iter()
                .position(|uid| *uid == user_id)
                .unwrap(); // Safe since it's in player_ranking but not flagged

            let (akai, iq) = match rank {
                0 => (20, 10),
                1 => (15, 7),
                2 => (10, 5),
                3 => (5, 3),
                4 => (2, 1),
                _ => (0, 0),
            };

            (
                format!(
                    "You ranked {} out of {} unflagged players.",
                    rank + 1,
                    unflagged_players.len()
                ),
                akai,
                iq,
            )
        };

        let mut metadata = HashMap::new();
        metadata.insert("akai_balance".to_string(), akai.to_string());
        metadata.insert("iq".to_string(), iq.to_string());
        metadata.insert("datapoint_id".to_string(), input.datapoint_id.clone());

        push_notification_to_user_do(
            env,
            user_id,
            NotificationType::Performance,
            &message,
            Some(metadata),
        )
        .await?;
    }

    Ok(())
}

