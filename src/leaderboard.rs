use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::{D1Database, Env, Request, Response, Result};

#[derive(Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub user_id: String,
    pub user_name: Option<String>,
    pub pfp: usize,
    pub product: usize,
    pub social_score: usize,
    pub iq: usize,
    pub king_lvl: usize,
    pub league: String,
}

#[derive(Deserialize)]
struct RangeQuery {
    p_min: Option<usize>,
    p_max: Option<usize>,
    user_id: Option<String>, // 🆕 for rank lookup
}

// Main handler for POST /api/leaderboard
pub async fn handle_leaderboard(mut req: Request, env: &Env) -> Result<Response> {
    let d1 = env.d1("D1_DATABASE")?;

    let body: Option<RangeQuery> = req.json().await.ok();
    let p_min = body.as_ref().and_then(|b| b.p_min).unwrap_or(0);
    let p_max = body.as_ref().and_then(|b| b.p_max).unwrap_or(usize::MAX);
    let user_id_opt = body.as_ref().and_then(|b| b.user_id.clone());

    let entries = get_players_in_range(&d1, p_min, p_max).await?;

    let user_rank = if let Some(user_id) = user_id_opt {
        Some(get_user_rank(&d1, p_min, p_max, &user_id).await?)
    } else {
        None
    };

    Response::from_json(&json!({
        "entries": entries,
        "user_rank": user_rank
    }))
}

async fn get_players_in_range(
    d1: &D1Database,
    p_min: usize,
    p_max: usize,
) -> Result<Vec<LeaderboardEntry>> {
    // Use COALESCE to handle potential NULL values
    let entries = d1
        .prepare(
            r#"
        SELECT 
            user_profile.user_id as "user_id",
            COALESCE(user_profile.user_name, '') as "user_name",
            COALESCE(user_profile.pfp, 0) as "pfp",
            progress.product as "product", 
            COALESCE(progress.social_score, 0) as "social_score",
            COALESCE(progress.iq, 0) as "iq",
            COALESCE(game_state.king_lvl, 0) as "king_lvl",
            COALESCE(user_data.league, 'bronze') as "league"
        FROM user_profile
        JOIN progress ON user_profile.user_id = progress.user_id
        JOIN game_state ON user_profile.user_id = game_state.user_id
        JOIN user_data ON user_profile.user_id = user_data.user_id
        WHERE progress.product BETWEEN ? AND ?
        ORDER BY progress.product DESC
        LIMIT 100
        "#,
        )
        .bind(&[p_min.into(), p_max.into()])?
        .all()
        .await?
        .results::<LeaderboardEntry>()?; // Direct deserialization

    Ok(entries)
}

async fn get_user_rank(
    d1: &D1Database,
    p_min: usize,
    p_max: usize,
    user_id: &str,
) -> Result<usize> {
    // Directly query and extract the rank as a primitive value
    let rank: usize = d1
        .prepare(
            r#"
        SELECT COUNT(DISTINCT product) + 1 as rank
        FROM progress
        WHERE product BETWEEN ?1 AND ?2
        AND product >= COALESCE(
            (SELECT product FROM progress WHERE user_id = ?3), 
            -1
        )
        AND user_id != ?3  -- Exclude the user themselves from the count
        "#,
        )
        .bind(&[p_min.into(), p_max.into(), user_id.into()])?
        .first::<usize>(Some("rank")) // Directly extract the "rank" column
        .await?
        .ok_or_else(|| worker::Error::RustError("User not found".to_string()))?;

    Ok(rank)
}
