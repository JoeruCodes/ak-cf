use futures::{stream::FuturesUnordered, StreamExt};
use wasm_bindgen::JsValue;
use worker::*;

use crate::{
    sql::update_user_data,
    types::{Op, UserData, WsMsg},
};

#[event(scheduled)]
async fn cron(event: ScheduledEvent, env: Env, ctx: ScheduleContext) {
    ctx.wait_until(run_cron_logic(env));
}

async fn run_cron_logic(env: Env) {
    console_log!("Cron job logic started.");

    let d1 = match env.d1("D1_DATABASE") {
        Ok(db) => db,
        Err(e) => {
            console_error!("Failed to get D1 binding: {}", e);
            return;
        }
    };

    let principals: Vec<String> = match get_all_user_ids(&d1).await {
        Ok(ids) => ids,
        Err(e) => {
            console_error!("Failed to retrieve user IDs for cron: {}", e);
            return;
        }
    };

    console_log!("Found {} users to process.", principals.len());

    let mut futures = FuturesUnordered::new();

    for user_id_str in principals.iter() {
        let env_clone = env.clone();
        let user_id_clone = user_id_str.clone();

        futures.push(async move {
            let d1_for_future = match env_clone.d1("D1_DATABASE") {
                Ok(db) => db,
                Err(e) => {
                    console_error!("Failed to get D1 binding for user {}: {}", user_id_clone, e);
                    return;
                }
            };

            console_log!("Processing user: {}", user_id_clone);
            let user_data_obj = match env_clone.durable_object("USER_DATA_WRAPPER") {
                Ok(obj) => obj,
                Err(e) => {
                    console_error!(
                        "Failed to get DO namespace for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            let user_data_id = match user_data_obj.id_from_name(&user_id_clone) {
                Ok(id) => {
                    console_log!("Got DO ID for user: {}", user_id_clone);
                    id
                }
                Err(e) => {
                    console_error!("Failed to get DO ID for user {}: {}", user_id_clone, e);
                    return;
                }
            };

            let user_data_stub = match user_data_id.get_stub() {
                Ok(stub) => {
                    console_log!("Got stub for DO ID: {}", user_data_id);
                    stub
                }
                Err(e) => {
                    console_error!("Failed to get stub for DO ID {}: {}", user_data_id, e);
                    return;
                }
            };

            let op_request = WsMsg {
                user_id: user_id_clone.clone(),
                op: Op::GetData,
            };

            let op_request_json = match serde_json::to_string(&op_request) {
                Ok(json) => {
                    console_log!("Serialized OpRequest for user: {}", user_id_clone);
                    json
                }
                Err(e) => {
                    console_error!(
                        "Failed to serialize OpRequest for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            let mut request_init = RequestInit::new();
            request_init.with_method(Method::Post);
            request_init.with_body(Some(JsValue::from_str(&op_request_json)));

            let request_url = "https://internal-do-fetch.com/";

            let request = match Request::new_with_init(request_url, &request_init) {
                Ok(req) => {
                    console_log!("Created request for user: {}", user_id_clone);
                    req
                }
                Err(e) => {
                    console_error!("Failed to create request for user {}: {}", user_id_clone, e);
                    return;
                }
            };

            let mut user_res = match user_data_stub.fetch_with_request(request).await {
                Ok(res) => {
                    console_log!("Fetched user data from DO for user: {}", user_id_clone);
                    res
                }
                Err(e) => {
                    console_error!(
                        "Failed to fetch user data from DO for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            let data: UserData = match user_res.json().await {
                Ok(json) => {
                    console_log!("Parsed JSON from DO for user: {}", user_id_clone);
                    json
                }
                Err(e) => {
                    console_error!(
                        "Failed to parse JSON from DO for user {}: {}",
                        user_id_clone,
                        e
                    );
                    return;
                }
            };

            match update_user_data(&data, &d1_for_future).await {
                Ok(_) => console_log!("Successfully updated D1 data for user {}", user_id_clone),
                Err(e) => {
                    console_error!("Failed to update D1 data for user {}: {}", user_id_clone, e)
                }
            }
        });
    }

    let mut count = 0;
    while let Some(_) = futures.next().await {
        count += 1;
    }

    console_log!("Processed {} user updates.", count);
    console_log!("Cron job logic finished.");
}

async fn get_all_user_ids(d1: &D1Database) -> Result<Vec<String>> {
    console_log!("Attempting to prepare D1 statement for JSON aggregation...");
    let statement = d1.prepare("SELECT JSON_GROUP_ARRAY(user_id) AS user_ids FROM user_profile");
    console_log!("D1 JSON aggregation statement prepared. Executing .first()...");

    #[derive(serde::Deserialize, Debug)]
    struct UserIdResult {
        user_ids: String,
    }

    let result_opt: Option<UserIdResult> = match statement.first::<UserIdResult>(None).await {
        Ok(opt_res) => {
            console_log!("D1 .first() executed successfully.");
            opt_res
        }
        Err(e) => {
            console_error!("D1 .first::<UserIdResult>() query failed: {}", e);
            return Err(e);
        }
    };

    match result_opt {
        Some(result_obj) => {
            console_log!("Successfully deserialized D1 result: {:?}", result_obj);
            let json_string = result_obj.user_ids;
            match serde_json::from_str::<Vec<String>>(&json_string) {
                Ok(ids) => {
                    console_log!(
                        "Successfully parsed JSON string into Vec<String> ({} users).",
                        ids.len()
                    );
                    Ok(ids)
                }
                Err(e) => {
                    console_error!("Failed to parse user_ids JSON string: {}", e);
                    Err(worker::Error::RustError(format!(
                        "Failed to parse D1 JSON result string: {}",
                        e
                    )))
                }
            }
        }
        None => {
            console_log!("D1 query returned no rows. Returning empty Vec.");
            Ok(Vec::new())
        }
    }
}
