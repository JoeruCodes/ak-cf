use std::collections::HashSet;

use candid::{Decode, Encode};
use futures::{stream::FuturesUnordered, StreamExt};
use ic_agent::{export::Principal, Agent};
use wasm_bindgen::JsValue;
use worker::*;

use crate::{Op, OpRequest, UserData};

#[event(scheduled)]
async fn cron(event: ScheduledEvent, env: Env, ctx: ScheduleContext) {
    // Initialize the Agent with proper error handling
    let agent = match Agent::builder().with_url("http://127.0.0.1:4943").build() {
        Ok(a) => a,
        Err(e) => {
            console_log!("Failed to build Agent: {}", e);
            return;
        }
    };

    // Fetch root key with error handling
    if let Err(e) = agent.fetch_root_key().await {
        console_log!("Failed to fetch root key: {}", e);
        return;
    }

    // Parse the canister ID with error handling
    let canister_id = match Principal::from_text("bkyz2-fmaaa-aaaaa-qaaaq-cai") {
        Ok(id) => id,
        Err(e) => {
            console_log!("Invalid canister ID: {}", e);
            return;
        }
    };

    // Query registered principals with error handling
    let principals = match agent
        .query(&canister_id, "get_registered_principals")
        .with_arg(Encode!(&()).unwrap())
        .call()
        .await
    {
        Ok(response) => match Decode!(&response, Vec<String>) {
            Ok(data) => data,
            Err(e) => {
                console_log!("Failed to decode principals: {}", e);
                return;
            }
        },
        Err(e) => {
            console_log!("Agent query failed: {}", e);
            return;
        }
    };

    let mut futures = FuturesUnordered::new();

    for cans_id in principals.iter() {
        let env_clone = env.clone();
        let agent_clone = agent.clone();
        let cans_id_clone = cans_id.clone();

        futures.push(async move {
            // Initialize the Durable Object stub with error handling
            let user_data_obj = env_clone.durable_object("USER_DATA_WRAPPER").unwrap();

            let user_data_id = match user_data_obj.id_from_name(&cans_id_clone) {
                Ok(id) => id,
                Err(e) => {
                    console_log!(
                        "Failed to get Durable Object ID for cans_id {}: {}",
                        cans_id_clone,
                        e
                    );
                    return;
                }
            };

            let user_data_stub = match user_data_id.get_stub() {
                Ok(stub) => stub,
                Err(e) => {
                    console_log!(
                        "Failed to get stub for Durable Object ID {}: {}",
                        user_data_id,
                        e
                    );
                    return;
                }
            };

            let op_request = OpRequest { op: Op::GetData };

            // Serialize the OpRequest to JSON
            let op_request_json = match serde_json::to_string(&op_request) {
                Ok(json) => json,
                Err(e) => {
                    console_log!("{:?}", e);
                    return;
                }
            };

            // Initialize the RequestInit with method, headers, and body
            let mut request_init = RequestInit::new();
            request_init.with_method(Method::Post);
            request_init.with_body(Some(JsValue::from_str(&op_request_json)));

            let request_url = "https://example.com/";

            // Create the Request object
            let request = match Request::new_with_init(request_url, &request_init) {
                Ok(req) => req,
                Err(e) => {
                    console_log!("{:?}", e);
                    return;
                }
            };

            // Fetch user data with error handling
            let mut user_res = match user_data_stub.fetch_with_request(request).await {
                Ok(res) => res,
                Err(e) => {
                    console_log!(
                        "Failed to fetch user data for cans_id {}: {}",
                        cans_id_clone,
                        e
                    );
                    return;
                }
            };

            let data: UserData = match user_res.json().await {
                Ok(json) => json,
                Err(e) => {
                    console_log!("Failed to parse JSON for cans_id {}: {}", cans_id_clone, e);
                    return;
                }
            };

            // Prepare the arguments for the update call with error handling
            let update_args = match Encode!(&cans_id_clone, &serde_json::to_string(&data).unwrap())
            {
                Ok(args) => args,
                Err(e) => {
                    console_log!(
                        "Failed to encode arguments for update call for cans_id {}: {}",
                        cans_id_clone,
                        e
                    );
                    return;
                }
            };

            // Send the agent update with error handling
            match agent_clone
                .update(&canister_id, "sync_updates")
                .with_arg(update_args)
                .call_and_wait()
                .await
            {
                Ok(_) => {
                    console_log!("Successfully updated agent for cans_id {}", cans_id_clone);
                }
                Err(e) => {
                    console_log!("Agent update failed for cans_id {}: {}", cans_id_clone, e);
                }
            }
        });
    }

    // Await all concurrent tasks and handle potential panics
    while let Some(_) = futures.next().await {
        // Individual tasks handle their own errors, so nothing is needed here
    }

    console_log!("Cron job completed.");
}
