use futures::TryStreamExt;
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng, Rng,
};
use registry::insert_new_user;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::JsValue;
use worker::*;

mod registry;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
enum Op {
    CombineAlien(usize, usize),
    SpawnAlien,
    DeleteAlienFromInventory(usize),
    DeleteAlienFromActive(usize),
    UsePowerup(usize),
    SpawnPowerup(PowerUpKind),
    GetData,
    Register,
    AwardBadge(BadgesKind),
    UpdateEmail(String),
    UpdatePfp(Option<String>),
    UpdateLastLogin(u64),
    UpdateIq(usize),
    UpdateSocialScore(usize),
    IncrementAkaiBalance,
    DecrementAkaiBalance,
    IncrementTotalTaskCompleted,
    IncrementPlayersReferred,
    UpdateLeague(LeagueType),
    UpdateAllTaskDone(bool),
}

#[derive(Serialize, Deserialize, Debug)]
struct WsMsg {
    user_id: String,
    op: Op,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy)]
enum PowerUpKind {
    RowPowerUp,
    ColumnPowerUp,
    NearestSquarePowerUp,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
enum BadgesKind {
    TenTaskBadge,
    TwentyTaskBadge,
    ThirtyTaskBadge,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
enum LeagueType {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Master,
    GrandMaster,
    Challenger,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
struct LeaderboardData {
    league: usize,
    global: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
struct UserProfile {
    user_id: String,
    email: Option<String>,
    pfp: Option<String>,
    last_login: u64,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
struct GameState {
    active_aliens: [usize; 16],
    inventory_aliens: Vec<usize>,
    power_ups: [Option<PowerUpKind>; 3],
    king_lvl: usize,
    total_merged_aliens: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
struct Progress {
    iq: usize,
    social_score: usize,
    product: usize,
    all_task_done: bool,
    akai_balance: usize,
    total_task_completed: usize,
    streak: usize,
    badges: Vec<BadgesKind>,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
struct SocialData {
    players_referred: usize,
    referal_code: String,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
struct UserData {
    profile: UserProfile,
    game_state: GameState,
    progress: Progress,
    social: SocialData,
    league: LeagueType,
}

// Provide a default implementation for UserData
impl Default for UserData {
    fn default() -> Self {
        console_log!("defaulting user data");
        Self {
            profile: UserProfile {
                user_id: Alphanumeric.sample_string(&mut thread_rng(), 32),
                email: None,
                pfp: None,
                last_login: Date::now().as_millis() / 1000,
            },
            game_state: GameState {
                active_aliens: [0; 16],
                inventory_aliens: Vec::new(),
                power_ups: [None; 3],
                king_lvl: 0,
                total_merged_aliens: 0,
            },
            progress: Progress {
                iq: 0,
                social_score: 0,
                all_task_done: false,
                product: 0,
                akai_balance: 0,
                total_task_completed: 0,
                streak: 0,
                badges: Vec::new(),
            },
            social: SocialData {
                players_referred: 0,
                referal_code: thread_rng()
                    .sample_iter(Alphanumeric)
                    .take(8)
                    .map(|b| b as char)
                    .collect(),
            },
            league: LeagueType::Bronze,
        }
    }
}
#[durable_object]
struct UserDataWrapper {
    state: State,
    env: Env,
}

fn calculate_product(user_data: &mut UserData) {
    user_data.progress.product =
        user_data.progress.iq * user_data.progress.social_score * user_data.game_state.king_lvl;
}

fn calculate_king_alien_lvl(user_data: &mut UserData) {
    user_data.game_state.king_lvl = user_data.game_state.active_aliens.iter().sum::<usize>();
}
#[durable_object]
impl DurableObject for UserDataWrapper {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        // Parse the incoming request as OpRequest
        let op_request: OpRequest = match req.json().await {
            Ok(op) => op,
            Err(e) => {
                console_log!("Failed to parse OpRequest: {:?}", e);
                return Response::error("Invalid request format", 400);
            }
        };

        // Retrieve user data from storage
        let mut user_data: UserData = self
            .state
            .storage()
            .get("user_data")
            .await
            .unwrap_or_default();
        console_log!("Host: {:?}", req.url());
        // Handle the operation

        console_log!("calculating streak");
        let current_time = Date::now().as_millis() / 1000;
        console_log!("current time: {:?}", current_time);
        let time_since_last_login = current_time - user_data.profile.last_login;
        let one_day = 60 * 60 * 24;
        let two_days = one_day * 2;

        if time_since_last_login > one_day && time_since_last_login < two_days {
            user_data.progress.streak += 1;
            user_data.profile.last_login = current_time;
        } else if time_since_last_login >= two_days {
            user_data.progress.streak = 0;
            user_data.profile.last_login = current_time;
        }

        let response = match &op_request.op {
            Op::CombineAlien(idx_a, idx_b) => {
                if idx_a != idx_b {
                    return Response::error("Combined Alien IDs are not the same", 400);
                }

                user_data.game_state.active_aliens[*idx_a] += 1;
                user_data.game_state.active_aliens[*idx_b] =
                    if !user_data.game_state.inventory_aliens.is_empty() {
                        user_data.game_state.inventory_aliens.pop().unwrap()
                    } else {
                        0
                    };

                user_data.game_state.total_merged_aliens += 1;
                calculate_king_alien_lvl(&mut user_data);
                Response::ok(
                    json!({
                        "active_aliens": user_data.game_state.active_aliens,
                        "inventory_aliens": user_data.game_state.inventory_aliens,
                        "total_merged_aliens": user_data.game_state.total_merged_aliens,
                        "king_lvl": user_data.game_state.king_lvl
                    })
                    .to_string(),
                )
            }
            Op::SpawnAlien => {
                let alien_lvl = user_data
                    .game_state
                    .active_aliens
                    .iter()
                    .max()
                    .unwrap_or(&5)
                    .max(&5)
                    - 4;
                if user_data.game_state.active_aliens.iter().all(|a| *a != 0) {
                    user_data.game_state.inventory_aliens.push(alien_lvl);
                } else {
                    for i in 0..user_data.game_state.active_aliens.len() {
                        if user_data.game_state.active_aliens[i] == 0 {
                            user_data.game_state.active_aliens[i] = alien_lvl;
                            break;
                        }
                    }
                    calculate_king_alien_lvl(&mut user_data);
                }
                Response::ok(
                    json!({
                        "active_aliens": user_data.game_state.active_aliens,
                        "inventory_aliens": user_data.game_state.inventory_aliens,
                        "total_merged_aliens": user_data.game_state.total_merged_aliens,
                        "king_lvl": user_data.game_state.king_lvl
                    })
                    .to_string(),
                )
            }
            Op::SpawnPowerup(powerup) => {
                if user_data
                    .game_state
                    .power_ups
                    .iter_mut()
                    .find(|p| p.is_none())
                    .map(|p| *p = Some(*powerup))
                    .is_none()
                {
                    Response::error("No empty slot for powerup", 400)
                } else {
                    Response::ok(
                        json!({
                            "power_ups": user_data.game_state.power_ups
                        })
                        .to_string(),
                    )
                }
            }
            Op::UsePowerup(idx) => {
                let power_up = user_data.game_state.power_ups[*idx];
                if power_up.is_none() {
                    return Response::error("No powerup in slot", 400);
                }
                power_up.map(|p| match p {
                    PowerUpKind::ColumnPowerUp => {
                        for i in 0..4 {
                            user_data.game_state.active_aliens[i] += 1;
                        }
                        user_data.game_state.power_ups[*idx] = None;
                    }
                    PowerUpKind::RowPowerUp => {
                        for i in 0..4 {
                            user_data.game_state.active_aliens[i * 4] += 1;
                        }
                        user_data.game_state.power_ups[*idx] = None;
                    }
                    PowerUpKind::NearestSquarePowerUp => {
                        for i in 0..4 {
                            user_data.game_state.active_aliens[i * 4] += 1;
                            user_data.game_state.active_aliens[i * 4 + 1] += 1;
                        }
                        user_data.game_state.power_ups[*idx] = None;
                    }
                });
                Response::ok(
                    json!({
                        "power_ups": user_data.game_state.power_ups
                    })
                    .to_string(),
                )
            }
            Op::AwardBadge(badge) => {
                user_data.progress.badges.push(badge.clone());
                Response::ok(
                    json!({
                        "badges": user_data.progress.badges
                    })
                    .to_string(),
                )
            }
            Op::GetData => Response::from_json(&user_data),
            Op::Register => {
                create_table_if_not_exists(&self.env.d1("D1_DATABASE")?).await?;
                match insert_new_user(&user_data, &self.env.d1("D1_DATABASE")?).await {
                    Ok(_) => Response::ok("User registered successfully!"),
                    Err(e) => {
                        console_error!("Registration failed: {:?}", e);
                        Response::error("Registration failed", 500)
                    }
                }
            }

            // Profile operations
            Op::UpdateEmail(email) => {
                user_data.profile.email = Some(email.clone());
                Response::ok(
                    json!({
                        "email": user_data.profile.email
                    })
                    .to_string(),
                )
            }
            Op::UpdatePfp(pfp) => {
                user_data.profile.pfp = pfp.clone();
                Response::ok(
                    json!({
                        "pfp": user_data.profile.pfp
                    })
                    .to_string(),
                )
            }
            Op::UpdateLastLogin(time) => {
                user_data.profile.last_login = *time;
                Response::ok(
                    json!({
                        "last_login": user_data.profile.last_login
                    })
                    .to_string(),
                )
            }

            // Progress operations
            Op::UpdateIq(iq) => {
                user_data.progress.iq = *iq;
                calculate_product(&mut user_data);
                Response::ok(
                    json!({
                        "iq": user_data.progress.iq
                    })
                    .to_string(),
                )
            }
            Op::UpdateSocialScore(score) => {
                user_data.progress.social_score = *score;
                calculate_product(&mut user_data);
                Response::ok(
                    json!({
                        "social_score": user_data.progress.social_score
                    })
                    .to_string(),
                )
            }
            Op::UpdateAllTaskDone(done) => {
                user_data.progress.all_task_done = *done;
                Response::ok(
                    json!({
                        "all_task_done": user_data.progress.all_task_done
                    })
                    .to_string(),
                )
            }
            Op::IncrementAkaiBalance => {
                user_data.progress.akai_balance += 1;
                Response::ok(
                    json!({
                        "akai_balance": user_data.progress.akai_balance
                    })
                    .to_string(),
                )
            }
            Op::DecrementAkaiBalance => {
                if user_data.progress.akai_balance > 0 {
                    user_data.progress.akai_balance -= 1;
                }
                Response::ok(
                    json!({
                        "akai_balance": user_data.progress.akai_balance
                    })
                    .to_string(),
                )
            }
            Op::IncrementTotalTaskCompleted => {
                user_data.progress.total_task_completed += 1;
                Response::ok(
                    json!({
                        "total_task_completed": user_data.progress.total_task_completed
                    })
                    .to_string(),
                )
            }

            // Social operations
            Op::IncrementPlayersReferred => {
                user_data.social.players_referred += 1;
                Response::ok(
                    json!({
                        "players_referred": user_data.social.players_referred
                    })
                    .to_string(),
                )
            }

            // League operations
            Op::UpdateLeague(league) => {
                user_data.league = league.clone();
                Response::ok(
                    json!({
                        "league": user_data.league
                    })
                    .to_string(),
                )
            }
            Op::DeleteAlienFromInventory(idx) => {
                user_data.game_state.inventory_aliens.remove(*idx);
                Response::ok(
                    json!({
                        "inventory_aliens": user_data.game_state.inventory_aliens
                    })
                    .to_string(),
                )
            }
            Op::DeleteAlienFromActive(idx) => {
                user_data.game_state.active_aliens[*idx] = 0;
                Response::ok(
                    json!({
                        "active_aliens": user_data.game_state.active_aliens
                    })
                    .to_string(),
                )
            }
        };

        // Save the updated user data if the operation modifies it
        if !matches!(op_request.op, Op::GetData) {
            if let Err(e) = self.state.storage().put("user_data", &user_data).await {
                console_log!("Storage put error: {:?}", e);
                return Response::error("Internal Server Error", 500);
            }
        }

        response
    }
}

#[derive(Serialize, Deserialize)]
struct OpRequest {
    op: Op,
}

// Entry point for handling fetch events (e.g., WebSocket upgrades)
#[event(fetch)]
pub async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    // Check if the request is a WebSocket upgrade
    if let Some(upgrade_header) = req.headers().get("Upgrade")? {
        let Some(auth_header) = req.headers().get("Authorization")? else {
            return Response::error("Unauthorized", 401);
        };

        if auth_header
            != env
                .var("AUTH_TOKEN")
                .map(|v| v.to_string())
                .unwrap_or("joel".to_string())
        {
            return Response::error("Unauthorized", 401);
        }
        if upgrade_header.to_lowercase() == "websocket" {
            // Create a WebSocket pair
            let pair = WebSocketPair::new()?;
            let client = pair.client;
            let server = WebSocket::from(pair.server);
            server.accept()?;

            // Clone the environment to use within the async task
            let env_clone = env.clone();
            // Spawn an asynchronous task to handle WebSocket events
            wasm_bindgen_futures::spawn_local(async move {
                // Obtain the event stream from the server WebSocket
                let mut events = match server.events() {
                    Ok(ev) => ev,
                    Err(e) => {
                        console_log!("Error obtaining event stream: {:?}", e);
                        return;
                    }
                };

                // Process incoming WebSocket events
                while let Some(event_result) = events.try_next().await.transpose() {
                    match event_result {
                        Ok(WebsocketEvent::Message(msg)) => {
                            // Attempt to parse the incoming message as JSON
                            let data: WsMsg = match msg.json() {
                                Ok(d) => d,
                                Err(e) => {
                                    console_log!("JSON parse error: {:?}", e);
                                    // Optionally, send an error message back to the client
                                    let _ = server.send_with_str(&format!("Error: {}", e));
                                    continue;
                                }
                            };

                            console_log!(
                                "Received operation {:?} from wallet_address: {}",
                                data.op,
                                data.user_id
                            );

                            // This call handles the operation and potential errors
                            match forward_op_to_do(&env_clone, &data).await {
                                Ok(mut res) => {
                                    // Always try to read the response body and send it back
                                    match res.text().await {
                                        Ok(response_text) => {
                                            console_log!(
                                                "Sending DO response back to client: {}",
                                                response_text
                                            );
                                            // Send the raw JSON string received from the DO
                                            if let Err(e) = server.send_with_str(&response_text) {
                                                console_error!(
                                                    "Error sending WebSocket message: {}",
                                                    e
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            // Error reading response body from DO
                                            let error_msg =
                                                format!("Error reading DO response body: {}", e);
                                            console_error!("{}", error_msg);
                                            if let Err(e) = server.send_with_str(&error_msg) {
                                                console_error!(
                                                    "Error sending WebSocket error message: {}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    console_log!("Failed to forward operation: {:?}", e);
                                    // Optionally, send an error message back to the client
                                    let error_msg = format!("Error processing operation: {}", e);
                                    if let Err(e_send) = server.send_with_str(&error_msg) {
                                        console_error!(
                                            "Error sending WebSocket error message: {}",
                                            e_send
                                        );
                                    }
                                }
                            }
                        }
                        Ok(WebsocketEvent::Close(close_event)) => {
                            console_log!("WebSocket closed: {:?}", close_event);
                            break;
                        }
                        Err(e) => {
                            console_log!("WebSocket event error: {:?}", e);
                            break;
                        }
                        _ => {} // Ignore other events
                    }
                }
            });

            return Response::from_websocket(client);
        }
    }

    Response::ok("This endpoint upgrades to WebSockets.")
}

async fn create_table_if_not_exists(d1: &D1Database) -> Result<Response> {
    // SQLite doesn't support ENUM types or array types, so we need to modify our approach
    let stmt = d1.prepare(
        r#"
    -- Create UserProfile table
    CREATE TABLE IF NOT EXISTS user_profile (
        user_id TEXT PRIMARY KEY,
        email TEXT,
        pfp TEXT,
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
    "#,
    );

    stmt.run().await?;
    Response::ok("Tables created successfully!")
}

// async fn register_wallet_address(wallet_address: &str, d1: D1Database) -> Result<Response> {
//     create_table_if_not_exists(&d1).await?;
//     console_log!("Tables created successfully!");
//     let mut default = UserData::default();
//     default.profile.user_id = wallet_address.to_string();
//     console_log!("Default user data created successfully!");
//     insert_new_user(&default, &d1).await?;
//     console_log!("User data inserted successfully!");
//     Response::ok("User registered successfully!")
// }

async fn forward_op_to_do(env: &Env, data: &WsMsg) -> Result<Response> {
    console_log!("Starting forward_op_to_do for user: {}", data.user_id);

    let do_namespace = env.durable_object("USER_DATA_WRAPPER")?;
    console_log!("Got durable object namespace");

    let do_id = do_namespace.id_from_name(&data.user_id)?;
    console_log!("Generated DO ID from user name");

    let do_stub = do_id.get_stub()?;
    console_log!("Got DO stub");

    let op_request = OpRequest {
        op: data.op.clone(),
    };

    // Serialize the OpRequest to JSON
    let op_request_json = match serde_json::to_string(&op_request) {
        Ok(json) => {
            console_log!("Successfully serialized op request to JSON");
            json
        }
        Err(e) => {
            console_log!("Failed to serialize op request: {:?}", e);
            return Err(worker::Error::RustError(format!(
                "Serialization error: {}",
                e
            )));
        }
    };

    // Initialize the RequestInit with method, headers, and body
    let mut request_init = RequestInit::new();
    request_init.with_method(Method::Post);
    request_init.with_body(Some(JsValue::from_str(&op_request_json)));
    console_log!("Initialized request with method and body");

    // Use a valid dummy URL
    let request_url = "https://example.com/";

    // Create the Request object
    let request = match Request::new_with_init(request_url, &request_init) {
        Ok(req) => {
            console_log!("Successfully created request object");
            req
        }
        Err(e) => {
            console_log!("Failed to create request object: {:?}", e);
            return Err(worker::Error::RustError(format!(
                "Request creation error: {:?}",
                e
            )));
        }
    };

    // Perform the fetch to the Durable Object
    let mut response = match do_stub.fetch_with_request(request).await {
        Ok(res) => {
            console_log!("Successfully fetched from DO");
            res
        }
        Err(e) => {
            console_log!("Failed to fetch from DO: {:?}", e);
            return Err(worker::Error::RustError(format!(
                "Fetch to Durable Object failed: {:?}",
                e
            )));
        }
    };

    // Handle the response from the Durable Object
    if response.status_code() != 200 {
        let error_message = match response.text().await {
            Ok(text) => text,
            Err(_) => "Unknown error".to_string(),
        };
        console_log!("Received error response from DO: {}", error_message);
        return Err(worker::Error::RustError(format!(
            "Durable Object Error: {}",
            error_message
        )));
    }

    console_log!("Successfully completed forward_op_to_do");
    Ok(response)
}
