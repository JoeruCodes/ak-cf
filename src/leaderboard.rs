use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::{D1Database, Env, Request, Response, Result};
use worker::{console_log}; // Make sure you import this
use worker::console_error;


#[derive(Serialize,Deserialize)]
pub struct LeaderboardEntry {
    pub user_id: String,
    pub user_name: Option<String>,
    pub pfp : usize,
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


// Fetch top 100 players in range
async fn get_players_in_range(
    d1: &D1Database,
    p_min: usize,
    p_max: usize,
) -> Result<Vec<LeaderboardEntry>> {
    let stmt = d1.prepare(
        r#"
        SELECT 
            user_profile.user_id, 
            user_profile.user_name, 
            user_profile.pfp,
            progress.product, 
            progress.social_score, 
            progress.iq, 
            game_state.king_lvl, 
            user_data.league
        FROM user_profile
        JOIN progress ON user_profile.user_id = progress.user_id
        JOIN game_state ON user_profile.user_id = game_state.user_id
        JOIN user_data ON user_profile.user_id = user_data.user_id
        WHERE progress.product BETWEEN ? AND ?
        ORDER BY progress.product DESC
        LIMIT 100
        "#,
    );

    #[derive(serde::Deserialize)]
    struct Row {
        user_id: String,
        user_name: Option<String>,
        pfp : usize,
        product: usize,
        social_score: usize,
        iq: usize,
        king_lvl: usize,
        league: String,
    }

    console_log!("{}",100);

    let rows: Vec<Row> = stmt
        .bind(&[p_min.into(), p_max.into()])?
        .all()
        .await?
        .results::<Row>()?;


        console_log!("{}",100);


    let entries = rows
        .into_iter()
        .map(|row| LeaderboardEntry {
            user_id: row.user_id,
            user_name: row.user_name,
            pfp : row.pfp,
            product: row.product,
            social_score: row.social_score,
            iq: row.iq,
            king_lvl: row.king_lvl,
            league: row.league,
        })
        .collect();

            console_log!("{}",100);


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
     AND product > COALESCE((SELECT product FROM progress WHERE user_id = ?), -1)"
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
