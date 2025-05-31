use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use sha2::{digest::Update, Digest};
use sql::UserCredentials;
use types::{DurableObjectAugmentedMsg, Op, UserData, WsMsg};
use utils::is_registered;
use wasm_bindgen::JsValue;
use worker::*;

mod daily_task;
mod gpt_voice;
mod leaderboard;
mod notification;
mod op_resolver;
mod registry;
mod sql;
mod types;
mod utils;

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
        let op_request: DurableObjectAugmentedMsg = match req.json().await {
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
            && !matches!(op_request.op, Op::Register(_))
        {
            return Response::error("User not registered", 400);
        } else if is_registered(&self.env.d1("D1_DATABASE").unwrap(), &op_request.user_id).await
            && matches!(op_request.op, Op::Register(_))
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

#[derive(Deserialize, Serialize)]
struct RegisterBody {
    user_id: String,
    password: String,
}

#[event(fetch)]
pub async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_log!("Fetching");
    let url = req.url()?;
    let path = url.path();

    console_log!("Path: {:?}", path);
    if path == "/api/leaderboard" {
        console_log!("Matched leaderboard route");
        if req.method() != Method::Get {
            return Response::error("Method Not Allowed", 405);
        }
        let result = leaderboard::handle_leaderboard(req, &env).await;
        console_log!("Leaderboard handler result: {:?}", result);
        return result;
    } else if path == "/api/register" {
        if req.method() != Method::Post {
            return Response::error("Method Not Allowed", 405);
        }
        let RegisterBody { user_id, password } = req.json().await?;
        let op = Op::Register(password);

        return forward_op_to_do(&env, &DurableObjectAugmentedMsg { user_id, op }).await;
    } else if path == "/api/notify_task_result" {
        if req.method() != Method::Post {
            return Response::error("Method Not Allowed", 405);
        }

        let input: notification::TaskResultInput = match req.json().await {
            Ok(data) => data,
            Err(_) => return Response::error("Invalid JSON", 400),
        };

        return match notification::notify_task_result(input, &env).await {
            Ok(_) => Response::ok("Notifications sent"),
            Err(e) => Response::error(format!("Failed: {}", e), 500),
        };
    } else if path == "/api/transcribe" {
        console_log!("Matched transcribe route");
        if req.method() != Method::Post {
            return Response::error("Method Not Allowed", 405);
        }
        return gpt_voice::handle_transcription(req, env).await;
    }

    console_log!("Not a leaderboard or register");

    if let Some(upgrade_header) = req.headers().get("Upgrade")? {
        let Some(username_header) = req.headers().get("username")? else {
            // If username header is missing, we can immediately return Unauthorized.
            console_log!("Unauthorized: Missing username");
            return Response::error("Unauthorized: Missing username", 401);
        };
        let Some(password_header) = req.headers().get("password")? else {
            console_log!("Unauthorized: Missing password");
            // If password header is missing, we can immediately return Unauthorized.
            return Response::error("Unauthorized: Missing password", 401);
        };

        console_log!(
            "Username header: {:?}, Password header: {:?}",
            username_header,
            password_header
        );
        // Authenticate against the database
        let db = env.d1("D1_DATABASE")?;
        console_log!("Database");
        match sql::get_user_credentials(&db, &username_header).await {
            Ok(Some(UserCredentials {
                user_id,
                user_name,
                password,
            })) => {
                let sha256 = sha2::Sha256::new();

                let password_header = sha256.chain(password_header.as_bytes()).finalize();

                if (user_name.unwrap_or(user_id)  == username_header) && password == hex::encode(password_header) {
                    // Credentials match, proceed with WebSocket upgrade
                } else {
                    // Password doesn't match
                    return Response::error("Unauthorized: Invalid credentials", 401);
                }
            }
            Ok(None) => {
                // User not found
                console_log!("Unauthorized: User not found");
                return Response::error("Unauthorized: User not found", 401);
            }
            Err(e) => {
                console_error!("Database error during authentication: {:?}", e);
                return Response::error("Internal Server Error", 500);
            }
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

                while let Some(event_result) = events.try_next().await.transpose() {
                    let user_id = username_header.clone();

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
                                "Received {:?} operation from {:?}",
                                data.op,
                                user_id.clone()
                            );

                            if matches!(data.op, Op::Register(_)) {
                                let _ = server.send_with_str(&format!(
                                    "Error: Register operation not allowed"
                                ));
                                continue;
                            }

                            match forward_op_to_do(
                                &env_clone,
                                &DurableObjectAugmentedMsg {
                                    user_id,
                                    op: data.op,
                                },
                            )
                            .await
                            {
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
                            console_log!("WebSocket connection closed for user: {}", user_id);
                            match forward_op_to_do(
                                &env_clone,
                                &DurableObjectAugmentedMsg {
                                    user_id: user_id.clone(),
                                    op: Op::SyncData,
                                },
                            )
                            .await
                            {
                                Ok(mut response) => {
                                    match response.text().await {
                                        Ok(text) => console_log!("SyncData on close for user {} successful: {}", user_id, text),
                                        Err(e) => console_error!("SyncData on close for user {}: failed to read response body: {}", user_id, e),
                                    }
                                }
                                Err(e) => {
                                    console_error!("SyncData on close for user {} failed: {}", user_id, e);
                                }
                            }
                            break;
                        }

                        Err(e) => {
                            console_error!(
                                "WebSocket event stream error for user {}: {}",
                                user_id,
                                e
                            );
                            match forward_op_to_do(
                                &env_clone,
                                &DurableObjectAugmentedMsg {
                                    user_id: user_id.clone(),
                                    op: Op::SyncData,
                                },
                            )
                            .await
                            {
                                Ok(mut response) => {
                                    match response.text().await {
                                        Ok(text) => console_log!("SyncData on error for user {} successful: {}", user_id, text),
                                        Err(e_resp) => console_error!("SyncData on error for user {}: failed to read response body: {}", user_id, e_resp),
                                    }
                                }
                                Err(e_fwd) => {
                                    console_error!("SyncData on error for user {} failed: {}", user_id, e_fwd);
                                }
                            }
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

async fn forward_op_to_do(env: &Env, data: &DurableObjectAugmentedMsg) -> Result<Response> {
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
