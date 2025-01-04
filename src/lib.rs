
use candid::Encode;
use futures::TryStreamExt;
use ic_agent::{export::Principal, Agent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::JsValue;
use worker::*;

mod registry;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
enum PowerUpKind {
    Spawner,
    ClickMultiplier,
    AutoFiller,
    AlienMultiplier,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum BadgesKind {
    LoginStreak { lvl: usize },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
enum Op {
    IncrementClick,
    IncrementShards(usize),
    CombineAlien(usize, usize),
    SpawnAlien(usize),
    UsePowerup(PowerUpKind),
    SpawnPowerup(PowerUpKind),
    GetData,
    Register
}

#[derive(Serialize, Deserialize, Debug)]
struct WsMsg {
    wallet_address: String,
    op: Op,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
struct UserData {
    #[serde(default)]
    name: Option<String>,
    wallet_address: String,
    clicks: usize,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    twitter: Option<String>,
    #[serde(default)]
    instagram: Option<String>,
    exp: usize,
    rating: usize,
    streak_count: usize,
    last_login: usize,
    #[serde(default)]
    aliens: Vec<usize>,
    #[serde(default)]
    power_ups: Vec<PowerUpKind>,
    #[serde(default)]
    badges: Vec<BadgesKind>,
}

// Provide a default implementation for UserData
impl Default for UserData {
    fn default() -> Self {
        Self {
            name: None,
            wallet_address: String::new(),
            clicks: 0,
            email: None,
            twitter: None,
            instagram: None,
            exp: 0,
            rating: 0,
            streak_count: 0,
            last_login: 0,
            aliens: vec![],
            power_ups: vec![],
            badges: vec![],
        }
    }
}

#[durable_object]
struct UserDataWrapper {
    state: State,
    env: Env,
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
        let mut user_data: UserData = self.state.storage().get("user_data").await.unwrap_or_default();
        console_log!("Host: {:?}", req.url());
        // Handle the operation
        let response = match &op_request.op {
            Op::IncrementClick => {
                user_data.clicks += 1;
                Response::ok("Incremented Click")
            }
            Op::IncrementShards(amount) => {
                user_data.exp += amount;
                Response::ok("Incremented Shards")
            }
            Op::CombineAlien(a, b) => {
                if a != b {
                    return Response::error("Combined Alien IDs are not the same", 400);
                }

                let index = user_data.aliens.iter().position(|alien| alien == a);
                if let Some(idx) = index {
                    user_data.aliens.remove(idx);
                    user_data.aliens.push(a + 1); // Example logic: increment alien level
                    Response::ok("Combined Alien")
                } else {
                    Response::error("Alien not found", 404)
                }
            }
            Op::SpawnAlien(lvl) => {
                if user_data.aliens.len() > 9 {
                    return Response::error("Aliens out of bounds", 403);
                }
                user_data.aliens.push(*lvl);
                Response::ok("Spawned Alien")
            }
            Op::SpawnPowerup(powerup) => {
                if user_data.power_ups.len() > 5 {
                    return Response::error("No more powerups can be given", 403);
                }

                user_data.power_ups.push(powerup.clone());
                match serde_json::to_string(powerup) {
                    Ok(json) => Response::ok(json),
                    Err(e) => {
                        console_log!("Serialization error: {:?}", e);
                        Response::error("Internal Server Error", 500)
                    }
                }
            }
            Op::UsePowerup(kind) => {
                if let Some(pos) = user_data.power_ups.iter().position(|p| *p == *kind) {
                    user_data.power_ups.remove(pos);
                    Response::ok("Powerup used")
                } else {
                    Response::error("No Powerup found", 400)
                }
            }
            Op::GetData => {
                Response::from_json(&user_data)
            },
            Op::Register => {
                register_wallet_address(&user_data.wallet_address).await.inspect_err(|e| console_log!("Registration failed :{:?}", e))
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
                                data.wallet_address
                            );


                            // Forward the operation to the Durable Object
                            if let Err(e) = forward_op_to_do(&env_clone, &data).await {
                                console_log!("Failed to forward operation: {:?}", e);
                                // Optionally, send an error message back to the client
                                let _ = server.send_with_str(&format!("Error: {}", e));
                            }

                            match forward_op_to_do(&env_clone, &data).await{
                                Ok(mut res) => {

                                    if data.op == Op::GetData{
                                        let response_body: UserData = res.json().await.unwrap();

                                        let _ = server.send_with_str(&json!(
                                            {"refreshed_data": response_body}
                                        ).to_string());
                                    }
                                },
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
    // Step 1: Build the Agent
    let agent = Agent::builder()
        .with_url("https://ic0.app").build()
        .map_err(|e| {
            console_log!("Failed to build agent: {:?}", e);
            e.to_string()
        })?;

    agent.fetch_root_key().await.unwrap();
    let principal = Principal::from_text("himj5-jiaaa-aaaag-atuna-cai").map_err(|e| {
        console_log!("Invalid Principal format: {:?}", e);
        e.to_string()
    })?;

    // Step 3: Encode the Argument
    let encoded_arg = Encode!(&wallet_address).map_err(|e| {
        console_log!("Failed to encode wallet address: {:?}", e);
        e.to_string()
    })?;

    // Step 4: Make the Update Call
    let res = agent.update(&principal, "register_principal")
        .with_arg(encoded_arg)
        .call_and_wait()
        .await
        .map_err(|e| {
            console_log!("Agent update call failed: {:?}", e);
            e.to_string()
        })?;

    // Optional: Handle the response `res` if needed
    console_log!("Agent update call succeeded: {:?}", res);

    Response::ok("Done!")
}


async fn forward_op_to_do(env: &Env, data: &WsMsg) -> Result<Response> {
    let do_namespace = env.durable_object("USER_DATA_WRAPPER")?;

    let do_id = do_namespace.id_from_name(&data.wallet_address)?;

    let do_stub = do_id.get_stub()?;

    let op_request = OpRequest {
        op: data.op.clone(),
    };

    // Serialize the OpRequest to JSON
    let op_request_json = match serde_json::to_string(&op_request) {
        Ok(json) => json,
        Err(e) => {
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

    // Use a valid dummy URL
    let request_url = "https://example.com/";

    // Create the Request object
    let request = match Request::new_with_init(request_url, &request_init) {
        Ok(req) => req,
        Err(e) => {
            return Err(worker::Error::RustError(format!(
                "Request creation error: {:?}",
                e
            )));
        }
    };

    // Perform the fetch to the Durable Object
    let mut response = match do_stub.fetch_with_request(request).await {
        Ok(res) => res,
        Err(e) => {
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
        return Err(worker::Error::RustError(format!(
            "Durable Object Error: {}",
            error_message
        )));
    }

    Ok(response)
}