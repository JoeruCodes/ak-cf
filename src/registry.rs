use futures::{stream::FuturesUnordered, StreamExt};
use serde_json;
use wasm_bindgen::JsValue;
use worker::*;

use crate::{BadgesKind, LeagueType, Op, OpRequest, PowerUpKind, UserData};

// Helper function to convert power_ups to JSON for SQLite
fn convert_power_ups_to_json(power_ups: &[Option<PowerUpKind>; 3]) -> String {
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
fn convert_badges_to_json(badges: &Vec<BadgesKind>) -> String {
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
fn league_to_string(league: &LeagueType) -> String {
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

// Helper function to convert Vec<String> or similar to JSON for SQLite
fn convert_string_vec_to_json(data: &Vec<String>) -> String {
    serde_json::to_string(data).unwrap_or_else(|_| "[]".to_string())
}

// NEW function for inserting a completely new user
pub async fn insert_new_user(data: &UserData, d1: &D1Database) -> Result<()> {
    let user_id = &data.profile.user_id;

    // Insert into user_profile
    let stmt_profile = d1
        .prepare("INSERT INTO user_profile (user_id, email, pfp, last_login) VALUES (?, ?, ?, ?)");
    stmt_profile
        .bind(&[
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
        d1.prepare("UPDATE user_profile SET email = ?, pfp = ?, last_login = ? WHERE user_id = ?");
    stmt_profile
        .bind(&[
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

#[event(scheduled)]
async fn cron(event: ScheduledEvent, env: Env, ctx: ScheduleContext) {
    // Pass the main async logic to ctx.wait_until()
    // This keeps the execution context alive until run_cron_logic completes.
    ctx.wait_until(run_cron_logic(env));
}

// Extracted core logic for the cron job
async fn run_cron_logic(env: Env) {
    console_log!("Cron job logic started.");

    let d1 = match env.d1("D1_DATABASE") {
        Ok(db) => db,
        Err(e) => {
            console_error!("Failed to get D1 binding: {}", e);
            return;
        }
    };

    // Call the helper function and handle its Result
    let principals: Vec<String> = match get_all_user_ids(&d1).await {
        Ok(ids) => ids,
        Err(e) => {
            console_error!("Failed to retrieve user IDs for cron: {}", e);
            return; // Stop cron if we can't get user IDs
        }
    };

    console_log!("Found {} users to process.", principals.len());

    let mut futures = FuturesUnordered::new();

    for user_id_str in principals.iter() {
        let env_clone = env.clone();
        let user_id_clone = user_id_str.clone();
        // Cannot clone D1Database, so get it from the cloned env inside the async block
        // let d1_clone = d1.clone(); // This is incorrect

        futures.push(async move {
            // Get D1 binding inside the future
            let d1_for_future = match env_clone.d1("D1_DATABASE") {
                Ok(db) => db,
                Err(e) => {
                    console_error!("Failed to get D1 binding for user {}: {}", user_id_clone, e);
                    return;
                }
            };

            console_log!("Processing user: {}", user_id_clone);
            // Initialize the Durable Object stub
            let user_data_obj = match env_clone.durable_object("USER_DATA_WRAPPER") {
                Ok(obj) => obj,
                Err(e) => {
                    console_error!(
                        "Failed to get DO namespace for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            let user_data_id = match user_data_obj.id_from_name(&user_id_clone) {
                Ok(id) => id,
                Err(e) => {
                    console_error!("Failed to get DO ID for user {}: {}", user_id_clone, e);
                    return;
                }
            };

            let user_data_stub = match user_data_id.get_stub() {
                Ok(stub) => stub,
                Err(e) => {
                    console_error!("Failed to get stub for DO ID {}: {}", user_data_id, e);
                    return;
                }
            };

            let op_request = OpRequest { op: Op::GetData };

            // Serialize the OpRequest to JSON
            let op_request_json = match serde_json::to_string(&op_request) {
                Ok(json) => json,
                Err(e) => {
                    console_error!(
                        "Failed to serialize OpRequest for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            // Initialize the RequestInit
            let mut request_init = RequestInit::new();
            request_init.with_method(Method::Post);
            request_init.with_body(Some(JsValue::from_str(&op_request_json)));

            let request_url = "https://internal-do-fetch.com/"; // Internal URL

            // Create the Request object
            let request = match Request::new_with_init(request_url, &request_init) {
                Ok(req) => req,
                Err(e) => {
                    console_error!("Failed to create request for user {}: {}", user_id_clone, e);
                    return;
                }
            };

            // Fetch user data from DO
            let mut user_res = match user_data_stub.fetch_with_request(request).await {
                Ok(res) => res,
                Err(e) => {
                    console_error!(
                        "Failed to fetch user data from DO for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            // Parse user data
            let data: UserData = match user_res.json().await {
                Ok(json) => json,
                Err(e) => {
                    console_error!(
                        "Failed to parse JSON from DO for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            // Update user data in D1 using the binding obtained within this future
            match update_user_data(&data, &d1_for_future).await {
                Ok(_) => console_log!("Successfully updated D1 data for user {}", user_id_clone),
                Err(e) => {
                    console_error!("Failed to update D1 data for user {}: {}", user_id_clone, e)
                }
            }
        });
    }

    // Await all concurrent tasks
    let mut count = 0;
    while let Some(_) = futures.next().await {
        count += 1;
        // Results are handled within each future
    }

    console_log!("Processed {} user updates.", count);
    console_log!("Cron job logic finished.");
}

// Helper function to encapsulate D1 query logic
async fn get_all_user_ids(d1: &D1Database) -> Result<Vec<String>> {
    console_log!("Attempting to prepare D1 statement for JSON aggregation...");
    let statement = d1.prepare("SELECT JSON_GROUP_ARRAY(user_id) AS user_ids FROM user_profile");
    console_log!("D1 JSON aggregation statement prepared. Executing .first()...");

    // Define struct matching the {user_ids: "[...]"} object returned by D1
    #[derive(serde::Deserialize, Debug)] // Added Debug for logging
    struct UserIdResult {
        // Ensure the field name matches the SQL alias
        user_ids: String,
    }

    // Use first::<UserIdResult>(None) to deserialize the result object directly
    let result_opt: Option<UserIdResult> = match statement.first::<UserIdResult>(None).await {
        Ok(opt_res) => {
            console_log!("D1 .first() executed successfully.");
            opt_res
        }
        Err(e) => {
            // Log the error details if possible
            console_error!("D1 .first::<UserIdResult>() query failed: {}", e);
            return Err(e);
        }
    };

    match result_opt {
        Some(result_obj) => {
            console_log!("Successfully deserialized D1 result: {:?}", result_obj);
            let json_string = result_obj.user_ids;
            // Parse the extracted JSON array string
            match serde_json::from_str::<Vec<String>>(&json_string) {
                Ok(ids) => {
                    console_log!(
                        "Successfully parsed JSON string into Vec<String> ({} users).",
                        ids.len()
                    );
                    Ok(ids)
                }
                Err(e) => {
                    console_error!("Failed to parse user_ids JSON string: {}", e);
                    Err(worker::Error::RustError(format!(
                        "Failed to parse D1 JSON result string: {}",
                        e
                    )))
                }
            }
        }
        None => {
            // Query returned no rows
            console_log!("D1 query returned no rows. Returning empty Vec.");
            Ok(Vec::new())
        }
    }
}
