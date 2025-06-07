use rand::seq::SliceRandom;
use rand::thread_rng;
use serde_json::Value;
use worker::D1Database;
use worker::*;

use crate::{
    sql,
    types::{BadgesKind, GameState, LeagueType, McqPreLabel, McqVideoTask, PowerUpKind, Question, TextVideoTask, UserData},
};

// Helper function to convert power_ups to JSON for SQLite
pub fn convert_power_ups_to_json(power_ups: &Vec<PowerUpKind>) -> String {
    let power_up_strings: Vec<Option<String>> = power_ups
        .iter()
        .map(|opt_pu| match opt_pu {
            PowerUpKind::RowPowerUp => Some("RowPowerUp".to_string()),
            PowerUpKind::ColumnPowerUp => Some("ColumnPowerUp".to_string()),
            PowerUpKind::NearestSquarePowerUp => Some("NearestSquarePowerUp".to_string()),
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
        user_data.progress.iq + user_data.progress.social_score * user_data.game_state.king_lvl;

    user_data.league = LeagueType::from_product(user_data.progress.product);
}

pub fn calculate_king_alien_lvl(user_data: &mut UserData) {
    // Calculate new level: (sum of active aliens / 50) + 1
    let sum: usize = user_data.game_state.active_aliens.iter().sum();
    let new_lvl = (sum / 50) + 1;

    // Only update if new level is higher than current level
    if new_lvl > user_data.game_state.king_lvl {
        user_data.game_state.king_lvl = new_lvl;

        // Add 50 to akai
        user_data.progress.akai_balance += 50;

        // Add 5 aliens (lvl - 3)
        for _ in 0..5 {
            let earned_alien = new_lvl * 10 - 3;

            let mut first_empty_index: Option<usize> = None;
            let mut min_value = usize::MAX;
            let mut min_index: usize = 0;

            for (i, &val) in user_data.game_state.active_aliens.iter().enumerate() {
                if val == 0 && first_empty_index.is_none() {
                    first_empty_index = Some(i);
                    break;
                }

                if val < min_value {
                    min_value = val;
                    min_index = i;
                }
            }

            let target_index = first_empty_index.unwrap_or(min_index);
            user_data.game_state.active_aliens[target_index] = earned_alien;
        }

        // Add a random power up
        let powerups = [
            PowerUpKind::RowPowerUp,
            PowerUpKind::ColumnPowerUp,
            PowerUpKind::NearestSquarePowerUp,
        ];

        let mut rng = thread_rng();
        let random_pu = *powerups.choose(&mut rng).unwrap();
        user_data.game_state.power_ups.push(random_pu);

        calculate_product(user_data); // 🧠 Update product only if level increased
    }
}

pub fn give_daily_reward(user_data: &mut UserData, index: usize) {
    if user_data.daily.total_completed >= 3 && user_data.daily.alien_earned.is_none() && index == 3
    {
        let earned_alien = user_data.game_state.king_lvl * 10 - 3;
        user_data.daily.alien_earned = Some(earned_alien);

        let mut first_empty_index: Option<usize> = None;
        let mut min_value = usize::MAX;
        let mut min_index: usize = 0;

        for (i, &val) in user_data.game_state.active_aliens.iter().enumerate() {
            if val == 0 && first_empty_index.is_none() {
                first_empty_index = Some(i);
                break;
            }

            if val < min_value {
                min_value = val;
                min_index = i;
            }
        }

        let target_index = first_empty_index.unwrap_or(min_index);
        user_data.game_state.active_aliens[target_index] = earned_alien;
        calculate_king_alien_lvl(user_data);
    }

    if user_data.daily.total_completed >= 5 && user_data.daily.pu_earned.is_none() && index == 5 {
        let powerups = [
            PowerUpKind::RowPowerUp,
            PowerUpKind::ColumnPowerUp,
            PowerUpKind::NearestSquarePowerUp,
        ];

        let mut rng = thread_rng();
        let random_pu = *powerups.choose(&mut rng).unwrap();

        user_data.daily.pu_earned = Some(random_pu);
        user_data.game_state.power_ups.push(random_pu);
    }
}

pub async fn fetch_mcq_video_tasks(n: usize, _env: &Env) -> Result<Vec<McqVideoTask>> {
    let url = "http://localhost:3001/api/game/fetch-mcq-datapoints"; // <-- Replace this
    let payload = serde_json::json!({ "numberOfDatapoints": n }).to_string();

    let req = Request::new_with_init(
        url,
        &RequestInit {
            method: Method::Post,
            body: Some(payload.into()),
            headers: {
                let mut headers = Headers::new();
                headers.set("Content-Type", "application/json")?;
                headers
            },
            ..Default::default()
        },
    )?;

    let mut response = Fetch::Request(req).send().await?;
    let data: serde_json::Value = response.json().await?;
    let datapoints = data["datapoints"].as_array().cloned().unwrap_or_default();

    let tasks = datapoints
        .iter()
        .map(|item| {
            let pre_label_val = &item["preLabel"];
            let questions_val = pre_label_val["questions"]
                .as_array()
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let keywords_val = pre_label_val["keywords"]
                .as_array()
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

            McqVideoTask {
                id: item["_id"].as_str().unwrap_or_default().to_string(),
                task_id: item["task_id"].as_str().unwrap_or_default().to_string(),
                mediaUrl: item["mediaUrl"].as_str().unwrap_or_default().to_string(),
                preLabel: McqPreLabel {
                    map_placement: pre_label_val["map_placement"]["value"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    summary: pre_label_val["summary"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    questions: questions_val
                        .iter()
                        .map(|q| Question {
                            q: q["q"].as_str().unwrap_or_default().to_string(),
                            a: q["a"].as_str().unwrap_or_default().to_string(),
                            textAnswers: vec![],
                            mcqAnswers: vec![],
                        })
                        .collect(),
                    keywords: keywords_val
                        .iter()
                        .filter_map(|kw| kw.as_str().map(String::from))
                        .collect(),
                },
                visited: false,
            }
        })
        .collect();

    Ok(tasks)
}

pub async fn fetch_text_video_tasks(n: usize, _env: &Env) -> Result<Vec<TextVideoTask>> {
    let url = "http://localhost:3001/api/game/fetch-textQ"; // <-- Replace this
    let payload = serde_json::json!({ "numberOfDatapoints": n }).to_string();

    let req = Request::new_with_init(
        url,
        &RequestInit {
            method: Method::Post,
            body: Some(payload.into()),
            headers: {
                let mut headers = Headers::new();
                headers.set("Content-Type", "application/json")?;
                headers
            },
            ..Default::default()
        },
    )?;

    let mut response = Fetch::Request(req).send().await?;
    let data: serde_json::Value = response.json().await?;
    let questions = data["questions"].as_array().cloned().unwrap_or_default();

    let tasks = questions
        .iter()
        .map(|item| TextVideoTask {
            datapointId: item["datapointId"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            questionIndex: item["questionIndex"].as_u64().unwrap_or(0) as usize,
            question: item["question"].as_str().unwrap_or_default().to_string(),
            mediaUrl: item["mediaUrl"].as_str().unwrap_or_default().to_string(),
            visited: false,
        })
        .collect();

    Ok(tasks)
}

#[derive(serde::Deserialize)]
struct UserIdRow {
    user_id: String,
}

pub async fn find_user_id_by_referral_code(d1: &D1Database, code: &str) -> Result<Option<String>> {
    let stmt = d1.prepare("SELECT user_id FROM social_data WHERE referal_code = ?");
    let res = stmt.bind(&[code.into()])?.first::<UserIdRow>(None).await;

    match res {
        Ok(Some(row)) => Ok(Some(row.user_id)),
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
} 