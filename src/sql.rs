use worker::{D1Database, Response, Result};

use crate::types::UserData;
use crate::utils::league_to_string;
use crate::utils::{convert_badges_to_json, convert_power_ups_to_json};
use crate::JsValue;

pub async fn create_table_if_not_exists(d1: &D1Database) -> Result<Response> {
    // SQLite doesn't support ENUM types or array types, so we need to modify our approach
    let stmt = d1.prepare(
        r#"
    -- Create UserProfile table
    CREATE TABLE IF NOT EXISTS user_profile (
        user_id TEXT PRIMARY KEY,
        email TEXT,
        pfp TEXT,
        user_name TEXT,
        password TEXT,
        last_login INTEGER NOT NULL
    );

    -- Create GameState table
    CREATE TABLE IF NOT EXISTS game_state (
        game_state_id INTEGER PRIMARY KEY AUTOINCREMENT,
        active_aliens TEXT NOT NULL, -- JSON string representing array
        inventory_aliens TEXT NOT NULL, -- JSON string representing array
        power_ups TEXT NOT NULL, -- JSON string representing array of enums
        king_lvl INTEGER NOT NULL,
        total_merged_aliens INTEGER NOT NULL,
        user_id TEXT NOT NULL,
        FOREIGN KEY (user_id) REFERENCES user_profile(user_id)
    );

    -- Create Progress table
    CREATE TABLE IF NOT EXISTS progress (
        progress_id INTEGER PRIMARY KEY AUTOINCREMENT,
        iq INTEGER NOT NULL,
        social_score INTEGER NOT NULL,
        product INTEGER NOT NULL,
        all_task_done INTEGER NOT NULL, -- SQLite boolean (0 or 1)
        akai_balance INTEGER NOT NULL,
        total_task_completed INTEGER NOT NULL,
        streak INTEGER NOT NULL,
        badges TEXT NOT NULL, -- JSON string representing array of enum values
        user_id TEXT NOT NULL,
        FOREIGN KEY (user_id) REFERENCES user_profile(user_id)
    );

    -- Create SocialData table
    CREATE TABLE IF NOT EXISTS social_data (
        social_data_id INTEGER PRIMARY KEY AUTOINCREMENT,
        players_referred INTEGER NOT NULL,
        referal_code TEXT NOT NULL,
        user_id TEXT NOT NULL,
        FOREIGN KEY (user_id) REFERENCES user_profile(user_id)
    );

    -- Create LeaderboardData table
    CREATE TABLE IF NOT EXISTS leaderboard_data (
        leaderboard_id INTEGER PRIMARY KEY AUTOINCREMENT,
        league INTEGER NOT NULL,
        global INTEGER NOT NULL,
        user_id TEXT NOT NULL,
        FOREIGN KEY (user_id) REFERENCES user_profile(user_id)
    );

    -- Create UserData table to link everything together
    CREATE TABLE IF NOT EXISTS user_data (
        user_id TEXT PRIMARY KEY,
        league TEXT NOT NULL, -- String representation of the enum
        FOREIGN KEY (user_id) REFERENCES user_profile(user_id)
    );

    -- Create Notifications table
CREATE TABLE IF NOT EXISTS notifications (
    notification_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    notification_type TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    read INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES user_profile(user_id)
);
    "#,
    );

    stmt.run().await?;
    Response::ok("Tables created successfully!")
}

pub async fn insert_new_user(data: &UserData, d1: &D1Database) -> Result<()> {
    let user_id = &data.profile.user_id;

    // Insert into user_profile
    let stmt_profile = d1
        .prepare("INSERT INTO user_profile (user_name,password, user_id, email, pfp, last_login) VALUES (?, ?, ?, ?, ?,?)");
    stmt_profile
        .bind(&[
            data.profile.user_name.clone().map(JsValue::from).unwrap_or(JsValue::null()),
            data.profile.password.clone().map(JsValue::from).unwrap_or(JsValue::null()),
            user_id.into(),
            data.profile
                .email
                .clone()
                .map(JsValue::from)
                .unwrap_or_else(JsValue::null),
            data.profile
                .pfp
                .clone()
                .map(JsValue::from)
                .unwrap_or_else(JsValue::null),
            (data.profile.last_login as f64).into(),
        ])?
        .run()
        .await?;

    // Insert into game_state
    let stmt_game = d1.prepare(
        "INSERT INTO game_state (user_id, active_aliens, inventory_aliens, power_ups, king_lvl, total_merged_aliens) VALUES (?, ?, ?, ?, ?, ?)"
    );
    let active_aliens_json =
        serde_json::to_string(&data.game_state.active_aliens).unwrap_or_else(|_| "[]".to_string());
    let inventory_aliens_json = serde_json::to_string(&data.game_state.inventory_aliens)
        .unwrap_or_else(|_| "[]".to_string());
    let power_ups_json = convert_power_ups_to_json(&data.game_state.power_ups);
    stmt_game
        .bind(&[
            user_id.into(),
            active_aliens_json.into(),
            inventory_aliens_json.into(),
            power_ups_json.into(),
            data.game_state.king_lvl.into(),
            data.game_state.total_merged_aliens.into(),
        ])?
        .run()
        .await?;

    // Insert into progress
    let stmt_progress = d1.prepare(
        "INSERT INTO progress (user_id, iq, social_score, product, all_task_done, akai_balance, total_task_completed, streak, badges) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    );
    let badges_json = convert_badges_to_json(&data.progress.badges);
    let all_task_done_int = if data.progress.all_task_done { 1 } else { 0 };
    stmt_progress
        .bind(&[
            user_id.into(),
            data.progress.iq.into(),
            data.progress.social_score.into(),
            data.progress.product.into(),
            all_task_done_int.into(),
            data.progress.akai_balance.into(),
            data.progress.total_task_completed.into(),
            data.progress.streak.into(),
            badges_json.into(),
        ])?
        .run()
        .await?;

    // Insert into social_data
    let stmt_social = d1.prepare(
        "INSERT INTO social_data (user_id, players_referred, referal_code) VALUES (?, ?, ?)",
    );
    stmt_social
        .bind(&[
            user_id.into(),
            data.social.players_referred.into(),
            data.social.referal_code.clone().into(),
        ])?
        .run()
        .await?;

    // Insert into user_data (linking table)
    let stmt_user_data = d1.prepare("INSERT INTO user_data (user_id, league) VALUES (?, ?)");
    let league_str = league_to_string(&data.league);
    stmt_user_data
        .bind(&[user_id.into(), league_str.into()])?
        .run()
        .await?;

    Ok(())
}

// MODIFIED function for updating existing user data (used by cron)
pub async fn update_user_data(data: &UserData, d1: &D1Database) -> Result<()> {
    let user_id = &data.profile.user_id;

    // Update user_profile
    let stmt_profile =
        d1.prepare("UPDATE user_profile SET user_name = ?,password = ?, email = ?, pfp = ?, last_login = ? WHERE user_id = ?");
    stmt_profile
        .bind(&[
            data.profile
                .user_name
                .clone()
                .map(JsValue::from)
                .unwrap_or_else(JsValue::null),
            data.profile
                .password
                .clone()
                .map(JsValue::from)
                .unwrap_or_else(JsValue::null),
            data.profile
                .email
                .clone()
                .map(JsValue::from)
                .unwrap_or_else(JsValue::null),
            data.profile
                .pfp
                .clone()
                .map(JsValue::from)
                .unwrap_or_else(JsValue::null),
            (data.profile.last_login as f64).into(),
            user_id.into(), // WHERE clause
        ])?
        .run()
        .await?;

    // Update game_state
    let stmt_game = d1.prepare(
        "UPDATE game_state SET active_aliens = ?, inventory_aliens = ?, power_ups = ?, king_lvl = ?, total_merged_aliens = ? WHERE user_id = ?"
    );
    let active_aliens_json =
        serde_json::to_string(&data.game_state.active_aliens).unwrap_or_else(|_| "[]".to_string());
    let inventory_aliens_json = serde_json::to_string(&data.game_state.inventory_aliens)
        .unwrap_or_else(|_| "[]".to_string());
    let power_ups_json = convert_power_ups_to_json(&data.game_state.power_ups);
    stmt_game
        .bind(&[
            active_aliens_json.into(),
            inventory_aliens_json.into(),
            power_ups_json.into(),
            data.game_state.king_lvl.into(),
            data.game_state.total_merged_aliens.into(),
            user_id.into(), // WHERE clause
        ])?
        .run()
        .await?;

    // Update progress
    let stmt_progress = d1.prepare(
        "UPDATE progress SET iq = ?, social_score = ?, product = ?, all_task_done = ?, akai_balance = ?, total_task_completed = ?, streak = ?, badges = ? WHERE user_id = ?"
    );
    let badges_json = convert_badges_to_json(&data.progress.badges);
    let all_task_done_int = if data.progress.all_task_done { 1 } else { 0 };
    stmt_progress
        .bind(&[
            data.progress.iq.into(),
            data.progress.social_score.into(),
            data.progress.product.into(),
            all_task_done_int.into(),
            data.progress.akai_balance.into(),
            data.progress.total_task_completed.into(),
            data.progress.streak.into(),
            badges_json.into(),
            user_id.into(), // WHERE clause
        ])?
        .run()
        .await?;

    // Update social_data
    let stmt_social = d1
        .prepare("UPDATE social_data SET players_referred = ?, referal_code = ? WHERE user_id = ?");
    stmt_social
        .bind(&[
            data.social.players_referred.into(),
            data.social.referal_code.clone().into(),
            user_id.into(), // WHERE clause
        ])?
        .run()
        .await?;

    // Update user_data (linking table)
    let stmt_user_data = d1.prepare("UPDATE user_data SET league = ? WHERE user_id = ?");
    let league_str = league_to_string(&data.league);
    stmt_user_data
        .bind(&[league_str.into(), user_id.into()])? // WHERE clause
        .run()
        .await?;

    Ok(())
}
