use uuid::Uuid;
use worker::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::{Env,JsValue};



#[derive(Deserialize, Clone, Debug, Serialize,PartialEq)]
pub struct Notification {
    pub notification_id: String,
    pub user_id: String,
    pub notification_type: NotificationType,
    pub message: String,
    pub timestamp: i64,
    pub read: Read,

    // ✅ New field for dynamic data
    pub metadata: Option<HashMap<String, String>>
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub enum NotificationType {
    Referral,
    Reward,
    System,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub enum Read{
    Yes,
    No
}

impl NotificationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationType::Referral => "Referral",
            NotificationType::Reward => "Reward",
            NotificationType::System => "System",
        }
    }
}

impl Read{
    pub fn as_str(&self) -> &'static str{
        match self{
            Read::Yes => "Yes",
            Read::No => "No",
            }
    }
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
