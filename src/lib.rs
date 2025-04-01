use std::{
    time::{SystemTime, UNIX_EPOCH},
    usize,
};

use candid::{CandidType, Encode};
use futures::TryStreamExt;
use ic_agent::{export::Principal, Agent};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::JsValue;
use worker::*;

mod registry;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy, CandidType)]
enum PowerUpKind {
    RowPowerUp,
    ColumnPowerUp,
    NearestSquarePowerUp,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, CandidType)]
enum BadgesKind {
    TenTaskBadge,
    TwentyTaskBadge,
    ThirtyTaskBadge,
}

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

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq, CandidType)]
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

#[derive(Deserialize, Clone, Debug, Serialize, CandidType)]
struct LeaderboardData {
    league: usize,
    global: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize, CandidType)]
struct UserProfile {
    user_id: Principal,
    email: Option<String>,
    pfp: Option<String>,
    last_login: u64,
}

#[derive(Deserialize, Clone, Debug, Serialize, CandidType)]
struct GameState {
    active_aliens: [usize; 16],
    inventory_aliens: Vec<usize>,
    power_ups: [Option<PowerUpKind>; 3],
    king_lvl: usize,
    total_merged_aliens: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize, CandidType)]
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

#[derive(Deserialize, Clone, Debug, Serialize, CandidType)]
struct SocialData {
    players_referred: usize,
    referal_code: String,
}

#[derive(Deserialize, Clone, Debug, Serialize, CandidType)]
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
                user_id: Principal::anonymous(),
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
            Op::Register => register_wallet_address(&user_data.profile.user_id.to_text())
                .await
                .inspect_err(|e| console_log!("Registration failed :{:?}", e)),

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

        if auth_header != env.var("AUTH_TOKEN").map(|v| v.to_string()).unwrap_or("joel".to_string()) {
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

                            // Forward the operation to the Durable Object
                            if let Err(e) = forward_op_to_do(&env_clone, &data).await {
                                console_log!("Failed to forward operation: {:?}", e);
                                // Optionally, send an error message back to the client
                                let _ = server.send_with_str(&format!("Error: {}", e));
                            }

                            match forward_op_to_do(&env_clone, &data).await {
                                Ok(mut res) => {
                                    if data.op == Op::GetData {
                                        let response_body: UserData = res.json().await.unwrap();

                                        let _ = server.send_with_str(
                                            &json!(
                                                {"refreshed_data": response_body}
                                            )
                                            .to_string(),
                                        );
                                    }
                                }
                                Err(e) => {
                                    console_log!("Failed to forward operation: {:?}", e);
                                    // Optionally, send an error message back to the client
                                    let _ = server.send_with_str(&format!("Error: {}", e));
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

async fn register_wallet_address(wallet_address: &str) -> Result<Response> {
    console_log!("Starting wallet registration for address: {}", wallet_address);
    
    // Step 1: Build the Agent
    let agent = Agent::builder()
        .with_url("http://127.0.0.1:4943")
        .build()
        .map_err(|e| {
            console_log!("Failed to build agent: {:?}", e);
            e.to_string()
        })?;

    console_log!("Agent built successfully");
    
    agent.fetch_root_key().await.unwrap();
    console_log!("Root key fetched");
    
    let principal = Principal::from_text("bkyz2-fmaaa-aaaaa-qaaaq-cai").map_err(|e| {
        console_log!("Invalid Principal format: {:?}", e);
        e.to_string()
    })?;
    console_log!("Principal parsed successfully");

    // Step 3: Encode the Argument
    let mut data = UserData::default();
    data.profile.user_id = Principal::from_text(wallet_address).map_err(|e| e.to_string())?;
    console_log!("User data initialized with wallet address");
    
    let encoded_arg = Encode!(&data).map_err(|e| {
        console_log!("Failed to encode wallet address: {:?}", e);
        e.to_string()
    })?;
    console_log!("Arguments encoded successfully");

    // Step 4: Make the Update Call
    console_log!("Making update call to register principal");
    let res = agent
        .update(&principal, "register_principal")
        .with_arg(encoded_arg)
        .call_and_wait()
        .await
        .map_err(|e| {
            console_log!("Agent update call failed: {:?}", e);
            e.to_string()
        })?;

    console_log!("Agent update call succeeded: {:?}", res);
    console_log!("Wallet registration completed successfully");

    Response::ok("Done!")
}

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
