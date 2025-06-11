use crate::{Env, JsValue};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use worker::*;
use crate::{types::{DurableObjectAugmentedMsg, Op}};

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

#[derive(Clone, Debug, PartialEq, Copy)]
pub struct RewardConfig {
    pub akai_on_correct: usize,
    pub akai_on_incorrect: usize,
    pub iq_on_correct: usize,
    pub iq_on_incorrect: isize,
}

pub fn get_reward_config(task_type: &str) -> Option<RewardConfig> {
    let mut map = HashMap::new();
    map.insert("mcq", RewardConfig { akai_on_correct: 10, akai_on_incorrect: 0, iq_on_correct: 5, iq_on_incorrect: -10 });
    map.insert("text", RewardConfig { akai_on_correct: 15, akai_on_incorrect: 0, iq_on_correct: 7, iq_on_incorrect: -10 });
    map.insert("audio", RewardConfig { akai_on_correct: 20, akai_on_incorrect: 0, iq_on_correct: 10, iq_on_incorrect: -3 });
    map.get(task_type).copied()
}

pub async fn push_notification_to_user_do(
    env: &Env,
    user_id: &str,
    notification_type: NotificationType,
    message: &str,
    metadata: Option<HashMap<String, String>>,
) -> Result<()> {
    let do_ns = env.durable_object("USER_DATA_WRAPPER")?;
    let do_id = do_ns.id_from_name(user_id)?;
    let stub = do_id.get_stub()?;

    let notif = Notification {
        notification_id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        notification_type,
        message: message.to_string(),
        timestamp: (Date::now().as_millis() / 1000) as i64,
        read: Read::No,
        metadata,
    };
    
    let do_msg = DurableObjectAugmentedMsg {
        user_id: user_id.to_string(),
        op: Op::AddNotificationInternal(notif),
    };

    let body = serde_json::to_string(&do_msg)
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    
    let mut request_init = RequestInit::new();
    request_init
        .with_method(Method::Post)
        .with_body(Some(wasm_bindgen::JsValue::from_str(&body)));
    
    let req = Request::new_with_init("https://do-internal/notification", &request_init)?;
    
    stub.fetch_with_request(req).await?;

    Ok(())
}

#[derive(Deserialize)]
struct ConsensusPayload {
    data: Vec<ConsensusData>,
}

#[derive(Deserialize)]
struct ConsensusData {
    user_id: String,
    flagged: bool,
    task_type: String,
}

pub async fn notify_task_result(mut req: Request, env: &Env) -> Result<Response> {
    let payload: ConsensusPayload = match req.json().await {
        Ok(p) => p,
        Err(e) => return Response::error(format!("Invalid JSON payload: {}", e), 400),
    };

    for user_data in payload.data {
        let is_correct = !user_data.flagged;
        
        if let Some(config) = get_reward_config(&user_data.task_type) {
            let (akai_reward, iq_change, message) = if is_correct {
                (
                    config.akai_on_correct, 
                    config.iq_on_correct as isize, 
                    format!("Task '{}' correct. Keep it up!", user_data.task_type)
                )
            } else {
                (
                    config.akai_on_incorrect, 
                    -(config.iq_on_incorrect as isize), 
                    format!("Task '{}' incorrect. This will result in loss of IQ.", user_data.task_type)
                )
            };

            let mut metadata = HashMap::new();
            metadata.insert("akai_balance".to_string(), akai_reward.to_string());
            metadata.insert("iq".to_string(), iq_change.to_string());

            if let Err(e) = push_notification_to_user_do(
                env,
                &user_data.user_id,
                NotificationType::Performance,
                &message,
                Some(metadata),
            ).await {
                console_error!("Failed to push notification for user {}: {:?}", user_data.user_id, e);
            }
        } else {
            console_log!("No reward config found for task_type: {}", user_data.task_type);
        }
    }

    Response::ok("Reward distribution processed.")
}

