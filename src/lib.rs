use futures::TryStreamExt;
use types::{Op, UserData, WsMsg};
use utils::is_registered;
use wasm_bindgen::JsValue;
use worker::*;

mod notification;
mod op_resolver;
mod registry;
mod sql;
mod types;
mod utils;
mod leaderboard;
mod daily_task;

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
        let op_request: WsMsg = match req.json().await {
            Ok(op) => op,
            Err(e) => {
                console_log!("Failed to parse OpRequest: {:?}", e);
                return Response::error("Invalid request format", 400);
            }
        };

        let mut user_data: UserData =
            self.state
                .storage()
                .get("user_data")
                .await
                .unwrap_or_else(|_| {
                    let mut user = UserData::default();
                    user.profile.user_id = op_request.user_id.clone();

                    // 🔁 Fix notification user_id
                    for notif in &mut user.notifications {
                        notif.user_id = op_request.user_id.clone();
                    }

                    user
                });
        console_log!("Host: {:?}", req.url());

        if !is_registered(&self.env.d1("D1_DATABASE").unwrap(), &op_request.user_id).await
            && op_request.op != Op::Register
        {
            return Response::error("User not registered", 400);
        } else if is_registered(&self.env.d1("D1_DATABASE").unwrap(), &op_request.user_id).await
            && op_request.op == Op::Register
        {
            return Response::error("User already registered", 400);
        }

        user_data.calculate_last_login();

        let response = user_data
            .resolve_op(&op_request, &self.env.d1("D1_DATABASE").unwrap(), &self.env)
            .await?;

        if !matches!(op_request.op, Op::GetData) {
            if let Err(e) = self.state.storage().put("user_data", &user_data).await {
                console_log!("Storage put error: {:?}", e);
                return Response::error("Internal Server Error", 500);
            }
        }

        Ok(response)
    }
}

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {

let url = req.url()?;
let path = url.path();

if path == "/api/leaderboard" {
    return leaderboard::handle_leaderboard(req, &env).await;
}


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
            let pair = WebSocketPair::new()?;
            let client = pair.client;
            let server = WebSocket::from(pair.server);
            server.accept()?;

            let env_clone = env.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut events = match server.events() {
                    Ok(ev) => ev,
                    Err(e) => {
                        console_log!("Error obtaining event stream: {:?}", e);
                        return;
                    }
                };

                let mut user_id = None;

                while let Some(event_result) = events.try_next().await.transpose() {
                    match event_result {
                        Ok(WebsocketEvent::Message(msg)) => {
                            let data: WsMsg = match msg.json() {
                                Ok(d) => d,
                                Err(e) => {
                                    console_log!("JSON parse error: {:?}", e);
                                    let _ = server.send_with_str(&format!("Error: {}", e));
                                    continue;
                                }
                            };

                            console_log!(
                                "Received operation {:?} from wallet_address: {}",
                                data.op,
                                data.user_id
                            );

                            if user_id.is_none(){
                                user_id = Some(data.user_id.clone());
                            }

                            match forward_op_to_do(&env_clone, &data).await {
                                Ok(mut res) => match res.text().await {
                                    Ok(response_text) => {
                                        console_log!(
                                            "Sending DO response back to client: {}",
                                            response_text
                                        );
                                        if let Err(e) = server.send_with_str(&response_text) {
                                            console_error!(
                                                "Error sending WebSocket message: {}",
                                                e
                                            );
                                        }
                                    }
                                    Err(e) => {
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
                                },
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
                        Ok(WebsocketEvent::Close(_)) => {

                            let Some(user_id) = &user_id else{
                                console_log!("No shit sherlock!");
                                break;
                            };

                            let res= forward_op_to_do(&env_clone, &WsMsg { user_id: user_id.clone(), op: Op::SyncData }).await;

                            console_log!("Sync res: {:?}", res);

                            break;
                        },
                        
                        Err(e) => {
                            console_log!("WebSocket event error: {:?}", e);
                            break;
                        }
                    }
                }
            });

            return Response::from_websocket(client);
        }
    }

    Response::ok("This endpoint upgrades to WebSockets.")
}

async fn forward_op_to_do(env: &Env, data: &WsMsg) -> Result<Response> {
    console_log!("Starting forward_op_to_do for user: {}", data.user_id);

    let do_namespace = env.durable_object("USER_DATA_WRAPPER")?;
    console_log!("Got durable object namespace");

    let do_id = do_namespace.id_from_name(&data.user_id)?;
    console_log!("Generated DO ID from user name");

    let do_stub = do_id.get_stub()?;
    console_log!("Got DO stub");

    let op_request_json = match serde_json::to_string(&data) {
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

    let mut request_init = RequestInit::new();
    request_init.with_method(Method::Post);
    request_init.with_body(Some(JsValue::from_str(&op_request_json)));
    console_log!("Initialized request with method and body");

    let request_url = "https://example.com/";

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
