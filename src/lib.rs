use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    user_id_for_session: Option<String>,
}

#[durable_object]
impl DurableObject for UserDataWrapper {
    fn new(state: State, env: Env) -> Self {
        Self { state, env, user_id_for_session: None }
    }

    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        let do_instance_hex_id = self.state.id().to_string();

        // Check for WebSocket upgrade request
        if req.headers().get("Upgrade")?.map_or(false, |h| h.to_lowercase() == "websocket") {
            console_log!("WebSocket upgrade request for DO instance (hex ID): {}", do_instance_hex_id);

            // Extract the username from the headers, similar to the main fetch logic.
            // This username is the human-readable ID.

            match req.headers().get("username")? {
                Some(username_from_header) => {
                    self.user_id_for_session = Some(username_from_header.clone());
                    // Persist user_id_for_session to storage
                    self.state.storage().put("user_id_for_session", username_from_header.clone()).await?;
                    console_log!(
                        "Set and stored user_id_for_session to: {} for DO instance {}",
                        username_from_header,
                        do_instance_hex_id
                    );

                    let pair = WebSocketPair::new()?;
                    let server_websocket = pair.server;
                    self.state.accept_web_socket(&server_websocket);
                    
                    console_log!("WebSocket connection accepted for user: {} (DO instance {})", username_from_header, do_instance_hex_id);
                    return Response::from_websocket(pair.client);
                }
                None => {
                    console_error!(
                        "CRITICAL: WebSocket upgrade for DO instance {} missing 'username' header.",
                        do_instance_hex_id
                    );
                    return Response::error("Unauthorized: Missing username header for WebSocket session", 401);
                }
            }
        }

        // Existing HTTP request handling logic
        console_log!("HTTP request for DO instance (hex ID): {}", do_instance_hex_id);
        let op_request: DurableObjectAugmentedMsg = match req.json().await {
            Ok(op) => op,
            Err(e) => {
                console_log!("Failed to parse OpRequest for DO instance {}: {:?}", do_instance_hex_id, e);
                return Response::error("Invalid request format", 400);
            }
        };

        // Use op_request.user_id as the logical user ID for operations and data.
        let logical_user_id = op_request.user_id.clone();
        console_log!("Processing operation for logical_user_id: {}", logical_user_id);

        // The check `if op_request.user_id != do_instance_hex_id` is removed as it compares name to hex_id.
        // The routing from main fetch ensures this DO instance corresponds to logical_user_id.

        let mut user_data: UserData =
            self.state
                .storage()
                .get("user_data")
                .await
                .unwrap_or_else(|err| {
                    console_log!(
                        "Failed to get user_data for logical_user_id {} (DO instance {}): {:?}",
                        logical_user_id,
                        do_instance_hex_id,
                        err
                    );
                    let mut user = UserData::default();
                    user.profile.user_id = logical_user_id.clone(); // Use human-readable ID
                    for notif in &mut user.notifications {
                        notif.user_id = logical_user_id.clone(); // Use human-readable ID
                    }
                    user
                });

        // Ensure consistency for existing data
        if user_data.profile.user_id != logical_user_id {
            user_data.profile.user_id = logical_user_id.clone();
        }
        for notif in &mut user_data.notifications {
            if notif.user_id != logical_user_id {
                notif.user_id = logical_user_id.clone();
            }
        }
        
        console_log!("Host for DO instance {}: {:?}", do_instance_hex_id, req.url());

        // Registration checks now use logical_user_id
        if !is_registered(&self.env.d1("D1_DATABASE").unwrap(), &logical_user_id).await
            && !matches!(op_request.op, Op::Register(_))
        {
            console_log!("User not registered: {}", logical_user_id);
            return Response::error("User not registered", 400);
        } else if is_registered(&self.env.d1("D1_DATABASE").unwrap(), &logical_user_id).await
            && matches!(op_request.op, Op::Register(_))
        {
            console_log!("User already registered: {}", logical_user_id);
            return Response::error("User already registered", 400);
        }

        user_data.calculate_last_login();

        let response = user_data
            .resolve_op(&op_request, &self.env.d1("D1_DATABASE").unwrap(), &self.env)
            .await?;

        if let Err(e) = self.state.storage().put("user_data", &user_data).await {
            console_log!("DO instance {}: Storage put error for {}: {:?}", do_instance_hex_id, logical_user_id, e);
            return Response::error("Internal Server Error", 500);
        }

        Ok(response)
    }

    async fn websocket_message(&mut self, ws: WebSocket, message: WebSocketIncomingMessage) -> Result<()> {
        self.ensure_user_id_for_session_loaded().await?;

        let do_name = match self.user_id_for_session.as_ref() {
            Some(id) => id.clone(),
            None => {
                console_error!(
                    "websocket_message called without user_id_for_session set (DO instance {}). This may happen if the session was not established correctly or data was lost.",
                    self.state.id().to_string()
                );
                let _ = ws.close(Some(1011), Some("Internal server error: Session context lost"));
                return Err(worker::Error::RustError("User ID not set in session".to_string()));
            }
        };

        console_log!("DO {}: Received WebSocket message", do_name);

        let message_bytes = match message {
            WebSocketIncomingMessage::String(text) => {
                // Assuming your WsMsg is JSON encoded, it typically comes as Text.
                text.into_bytes()
            }
            WebSocketIncomingMessage::Binary(bytes) => {
                // If WsMsg could be in a binary format, handle appropriately.
                bytes
            }
        };

        let data: WsMsg = match serde_json::from_slice(&message_bytes) {
            Ok(d) => d,
            Err(e) => {
                console_log!("DO {}: JSON parse error from WebSocket: {:?}", do_name, e);
                let _ = ws.send(&format!("Error: JSON parse error: {}", e));
                return Ok(()); // Don't terminate connection for bad message, just inform client
            }
        };

        console_log!(
            "DO {}: Received {:?} operation from WebSocket",
            do_name,
            data.op
        );

        if matches!(data.op, Op::Register(_)) {
            console_log!("DO {}: Register operation not allowed over WebSocket", do_name);
            let _ = ws.send(&"Error: Register operation not allowed over WebSocket");
            return Ok(());
        }

        let mut user_data: UserData = self.state.storage().get("user_data").await.unwrap_or_else(|err| {
            console_log!(
                "DO {}: Failed to get user_data in websocket_message (normal for new user): {:?}",
                do_name,
                err
            );
            let mut user = UserData::default();
            user.profile.user_id = do_name.clone();
            for notif in &mut user.notifications {
                notif.user_id = do_name.clone();
            }
            user
        });

        // Ensure consistency
        if user_data.profile.user_id != do_name {
            user_data.profile.user_id = do_name.clone();
        }
        for notif in &mut user_data.notifications {
            if notif.user_id != do_name {
                notif.user_id = do_name.clone();
            }
        }

        user_data.calculate_last_login();
        
        let op_msg = DurableObjectAugmentedMsg {
            user_id: do_name.clone(),
            op: data.op,
        };

        match user_data.resolve_op(&op_msg, &self.env.d1("D1_DATABASE")?, &self.env).await {
            Ok(mut res) => match res.json::<Value>().await {
                Ok(response_text) => {
                    console_log!(
                        "DO {}: Sending op response back to client: {}",
                        do_name,
                        response_text
                    );
                    if let Err(e) = ws.send(&response_text) {
                        console_error!("DO {}: Error sending WebSocket message: {}", do_name, e);
                    }
                }
                Err(e) => {
                    let error_msg = format!("Error reading DO response body: {}", e);
                    console_error!("DO {}: {}", do_name, error_msg);
                    if let Err(e_send) = ws.send(&error_msg) {
                        console_error!("DO {}: Error sending WebSocket error message: {}", do_name, e_send);
                    }
                }
            },
            Err(e) => {
                console_log!("DO {}: Failed to resolve op via WebSocket: {:?}", do_name, e);
                let error_msg = format!("Error processing operation: {}", e);
                if let Err(e_send) = ws.send(&error_msg) {
                    console_error!("DO {}: Error sending WebSocket error message: {}", do_name, e_send);
                }
            }
        }

        if let Err(e) = self.state.storage().put("user_data", &user_data).await {
            console_error!("DO {}: Storage put error after websocket_message: {:?}", do_name, e);
            // Optionally inform client if appropriate, though primary op response is more critical
        }

        Ok(())
    }

    async fn websocket_close(&mut self, _ws: WebSocket, _code: usize, _reason: String, _was_clean: bool) -> Result<()> {
        self.ensure_user_id_for_session_loaded().await?;

        let do_name = match self.user_id_for_session.as_ref() {
            Some(id) => id.clone(),
            None => {
                console_error!(
                    "websocket_close called without user_id_for_session set (DO instance {}). Cannot perform cleanup or sync accurately.",
                    self.state.id().to_string()
                );
                // Attempt to delete from storage anyway, in case it's there but wasn't loaded.
                let _ = self.state.storage().delete("user_id_for_session").await;
                return Err(worker::Error::RustError("User ID not set in session on close".to_string()));
            }
        };
        console_log!("DO {}: WebSocket connection closed.", do_name);
        self.sync_data_on_disconnect(&do_name).await;

        // Clean up stored session ID
        if let Err(e) = self.state.storage().delete("user_id_for_session").await {
            console_error!("DO {}: Failed to delete user_id_for_session from storage on close: {:?}", do_name, e);
        }
        self.user_id_for_session = None; // Clear in-memory field

        Ok(())
    }

    async fn websocket_error(&mut self, _ws: WebSocket, _error: Error) -> Result<()> {
        self.ensure_user_id_for_session_loaded().await?;

        let do_name = match self.user_id_for_session.as_ref() {
            Some(id) => id.clone(),
            None => {
                console_error!(
                    "websocket_error called without user_id_for_session set (DO instance {}). Error: {:?}",
                    self.state.id().to_string(),
                    _error
                );
                // Attempt to delete from storage anyway.
                let _ = self.state.storage().delete("user_id_for_session").await;
                return Err(worker::Error::RustError("User ID not set in session on error".to_string()));
            }
        };
        console_error!("DO {}: WebSocket error: {:?}", do_name, _error);
        self.sync_data_on_disconnect(&do_name).await;

        // Clean up stored session ID
        if let Err(e) = self.state.storage().delete("user_id_for_session").await {
            console_error!("DO {}: Failed to delete user_id_for_session from storage on error: {:?}", do_name, e);
        }
        self.user_id_for_session = None; // Clear in-memory field

        Ok(())
    }
}

// Helper methods for UserDataWrapper
impl UserDataWrapper {
    // Helper method to ensure user_id_for_session is loaded if the DO was rehydrated
    async fn ensure_user_id_for_session_loaded(&mut self) -> Result<()> {
        if self.user_id_for_session.is_none() {
            if let Some(stored_id) = self.state.storage().get("user_id_for_session").await? {
                console_log!(
                    "DO instance {}: Rehydrated user_id_for_session from storage: {}",
                    self.state.id().to_string(),
                    stored_id
                );
                self.user_id_for_session = Some(stored_id);
            }
        }
        Ok(())
    }

    // Helper function for SyncData on disconnect
    async fn sync_data_on_disconnect(&mut self, do_name: &str) {
        console_log!("DO {}: Attempting SyncData on disconnect.", do_name);
        let mut user_data: UserData = self.state.storage().get("user_data").await.unwrap_or_else(|err| {
            console_warn!(
                "DO {}: Failed to get user_data for SyncData (may not exist if no ops were run): {:?}",
                do_name,
                err
            );
            // If user_data doesn't exist, creating a default one just for SyncData might not be intended
            // unless SyncData op can handle a totally new user gracefully.
            // For now, let's assume SyncData op is safe with default or if it does nothing for new user.
            let mut user = UserData::default();
            user.profile.user_id = do_name.to_string();
            for notif in &mut user.notifications {
                notif.user_id = do_name.to_string();
            }
            user
        });

        // Ensure consistency, though it should be if session was active
        if user_data.profile.user_id != do_name {
            user_data.profile.user_id = do_name.to_string();
        }
         for notif in &mut user_data.notifications {
            if notif.user_id != do_name {
                notif.user_id = do_name.to_string();
            }
        }

        let op_msg = DurableObjectAugmentedMsg {
            user_id: do_name.to_string(),
            op: Op::SyncData, // Assuming Op::SyncData exists
        };

        match user_data.resolve_op(&op_msg, &self.env.d1("D1_DATABASE").unwrap(), &self.env).await {
            Ok(_) => {
                console_log!("DO {}: SyncData on disconnect successful.", do_name);
                if let Err(e) = self.state.storage().put("user_data", &user_data).await {
                    console_error!("DO {}: Storage put error after SyncData: {:?}", do_name, e);
                }
            }
            Err(e) => {
                console_error!(
                    "DO {}: SyncData on disconnect failed to resolve op: {:?}",
                    do_name,
                    e
                );
            }
        }
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
    } else if path == "/api/notify_task_result" {
        if req.method() != Method::Post {
            return Response::error("Method Not Allowed", 405);
        }
        return notification::notify_task_result(req, &env).await;
    } else if path == "/api/register" {
        if req.method() != Method::Post {
            return Response::error("Method Not Allowed", 405);
        }
        let RegisterBody { user_id, password } = req.json().await?;
        let op = Op::Register(password);

        // Updated logic to call the DO directly
        let do_ns = env.durable_object("USER_DATA_WRAPPER")?;
        // Use the user_id from the registration body to name/get the DO
        let do_id = do_ns.id_from_name(&user_id)?;
        let do_stub = do_id.get_stub()?;
        
        let op_msg = DurableObjectAugmentedMsg { user_id, op }; // user_id is from RegisterBody
        
        let op_request_json = match serde_json::to_string(&op_msg) {
            Ok(json) => json,
            Err(e) => {
                console_error!("Failed to serialize Op::Register message: {:?}", e);
                return Response::error("Internal Server Error: Failed to create registration request", 500);
            }
        };

        let mut request_init = RequestInit::new();
        request_init.with_method(Method::Post);
        request_init.with_body(Some(JsValue::from_str(&op_request_json)));
        
        // The URL for the DO request can be a placeholder as it's not used for routing when calling a stub directly.
        let do_req = match Request::new_with_init("https://do-internal/register", &request_init) {
            Ok(r) => r,
            Err(e) => {
                console_error!("Failed to create Request for DO (Op::Register): {:?}", e);
                return Response::error("Internal Server Error: Failed to create registration sub-request", 500);
            }
        };
        
        return do_stub.fetch_with_request(do_req).await;
    } else if path == "/api/transcribe" {
        console_log!("Matched transcribe route");
        if req.method() != Method::Post {
            return Response::error("Method Not Allowed", 405);
        }
        return gpt_voice::handle_transcription(req, env).await;
    }

    console_log!("Not a leaderboard or register");

    if let Some(upgrade_header) = req.headers().get("Upgrade")? {
        let Some( mut username_header) = req.headers().get("username")? else {
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

                if (user_name.as_ref().map(|s| s == &username_header).unwrap_or(false)
    || user_id == username_header)
    && password == hex::encode(password_header) {
                    // Credentials match, proceed with WebSocket upgrade
                    
                } else {

                    // Password doesn't match
                    return Response::error("Unauthorized: Invalid credentials", 401);
                }
                username_header=user_id.clone();
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
                console_log!("Database");


        if upgrade_header.to_lowercase() == "websocket" {
            let do_namespace = env.durable_object("USER_DATA_WRAPPER")?;
            let do_id = do_namespace.id_from_name(&username_header)?; // username_header is the authenticated user_id
            let do_stub = do_id.get_stub()?;

            console_log!("Forwarding WebSocket request for user {} to DO", username_header);
            return do_stub.fetch_with_request(req).await; // Forward the original request
        }
    }

    Response::ok("This endpoint upgrades to WebSockets.")
}
