use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::{D1Database, Env, Request, Response, Result};

#[derive(Serialize)]
pub struct LeaderboardEntry {
    pub user_id: String,
    pub product: usize,
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

// Fetch top 100 players in range
async fn get_players_in_range(
    d1: &D1Database,
    p_min: usize,
    p_max: usize,
) -> Result<Vec<LeaderboardEntry>> {
    let stmt = d1.prepare(
        "SELECT user_profile.user_id, product
         FROM user_profile
         JOIN progress ON user_profile.user_id = progress.user_id
         WHERE product BETWEEN ? AND ?
         ORDER BY product DESC
         LIMIT 100",
    );

    #[derive(serde::Deserialize)]
    struct Row {
        user_id: String,
        product: usize,
    }

    let rows: Vec<Row> = stmt
        .bind(&[p_min.into(), p_max.into()])?
        .all()
        .await?
        .results::<Row>()?;

    let entries = rows
        .into_iter()
        .map(|row| LeaderboardEntry {
            user_id: row.user_id,
            product: row.product,
        })
        .collect();

    Ok(entries)
}

// Get user rank even if outside top 100
async fn get_user_rank(
    d1: &D1Database,
    p_min: usize,
    p_max: usize,
    user_id: &str,
) -> Result<usize> {
    let stmt = d1.prepare(
        "SELECT COUNT(*) + 1 as rank
         FROM progress
         WHERE product BETWEEN ? AND ?
         AND product > (SELECT product FROM progress WHERE user_id = ?)",
    );

    #[derive(serde::Deserialize)]
    struct RankRow {
        rank: usize,
    }

    let row: RankRow = stmt
        .bind(&[p_min.into(), p_max.into(), user_id.into()])?
        .first::<RankRow>(None)
        .await?
        .ok_or_else(|| worker::Error::RustError("User not found".to_string()))?;

    Ok(row.rank)
}
