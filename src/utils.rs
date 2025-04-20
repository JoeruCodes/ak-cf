use serde_json::Value;
use worker::D1Database;

use crate::{
    sql,
    types::{BadgesKind, LeagueType, PowerUpKind, UserData},
};

// Helper function to convert power_ups to JSON for SQLite
pub fn convert_power_ups_to_json(power_ups: &[Option<PowerUpKind>; 3]) -> String {
    let power_up_strings: Vec<Option<String>> = power_ups
        .iter()
        .map(|opt_pu| match opt_pu {
            Some(PowerUpKind::RowPowerUp) => Some("RowPowerUp".to_string()),
            Some(PowerUpKind::ColumnPowerUp) => Some("ColumnPowerUp".to_string()),
            Some(PowerUpKind::NearestSquarePowerUp) => Some("NearestSquarePowerUp".to_string()),
            None => None,
        })
        .collect();

    serde_json::to_string(&power_up_strings).unwrap_or_else(|_| "[null,null,null]".to_string())
}

// Helper function to convert badges to JSON for SQLite
pub fn convert_badges_to_json(badges: &Vec<BadgesKind>) -> String {
    if badges.is_empty() {
        return "[]".to_string();
    }

    let badge_strings: Vec<String> = badges
        .iter()
        .map(|badge| match badge {
            BadgesKind::TenTaskBadge => "TenTaskBadge".to_string(),
            BadgesKind::TwentyTaskBadge => "TwentyTaskBadge".to_string(),
            BadgesKind::ThirtyTaskBadge => "ThirtyTaskBadge".to_string(),
        })
        .collect();

    serde_json::to_string(&badge_strings).unwrap_or_else(|_| "[]".to_string())
}

// Helper function to convert LeagueType to string
pub fn league_to_string(league: &LeagueType) -> String {
    match league {
        LeagueType::Bronze => "Bronze",
        LeagueType::Silver => "Silver",
        LeagueType::Gold => "Gold",
        LeagueType::Platinum => "Platinum",
        LeagueType::Diamond => "Diamond",
        LeagueType::Master => "Master",
        LeagueType::GrandMaster => "GrandMaster",
        LeagueType::Challenger => "Challenger",
    }
    .to_string()
}

pub async fn is_registered(d1: &D1Database, user_id: &str) -> bool {
    sql::create_table_if_not_exists(d1)
        .await
        .expect("creation failed");
    let stmt = d1.prepare("SELECT 1 FROM user_profile WHERE user_id = ?");
    stmt.bind(&[user_id.into()])
        .expect("bind failed")
        .run()
        .await
        .expect("run failed")
        .results::<Value>()
        .expect("results failed")
        .len()
        > 0
}

pub fn calculate_product(user_data: &mut UserData) {
    user_data.progress.product =
        user_data.progress.iq * user_data.progress.social_score * user_data.game_state.king_lvl;
}

pub fn calculate_king_alien_lvl(user_data: &mut UserData) {
    user_data.game_state.king_lvl = user_data.game_state.active_aliens.iter().sum::<usize>();
}
