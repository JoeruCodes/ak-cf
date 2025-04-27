use uuid::Uuid;
use worker::{D1Database, Result};
use chrono::Utc;


#[derive(Debug, Clone)]
pub struct Notification {
    pub notification_id: String,
    pub user_id: String,
    pub notification_type: NotificationType,
    pub message: String,
    pub timestamp: i64,
    pub read: bool,
}

#[derive(Debug, Clone)]
pub enum NotificationType {
    Referral,
    Reward,
    System,
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

pub async fn add_notification(
    d1: &D1Database,
    user_id: &str,
    notification_type: NotificationType,
    message: &str,
) -> Result<()> {
    let notification_id = Uuid::new_v4().to_string();
    let timestamp = Utc::now().timestamp();

    let stmt = d1.prepare(
        "INSERT INTO notifications (notification_id, user_id, notification_type, message, timestamp, read) VALUES (?, ?, ?, ?, ?, ?)"
    );

    stmt.bind(&[
        notification_id.into(),
        user_id.into(),
        notification_type.as_str().into(),
        message.into(),
        (timestamp as f64).into(),
        0.into(),
    ])?
    .run()
    .await?;

    Ok(())
}