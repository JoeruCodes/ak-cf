use serde::Deserialize;
use worker::*;

// Structure for the response from OpenAI's transcription API
#[derive(Deserialize, Debug)]
struct TranscriptionResponse {
    text: String,
}

// Structure for the error response from OpenAI's API
#[derive(Deserialize, Debug)]
struct OpenAIErrorResponse {
    error: OpenAIErrorDetails,
}

#[derive(Deserialize, Debug)]
struct OpenAIErrorDetails {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
    param: Option<String>,
    code: Option<String>,
}

pub async fn handle_transcription(mut req: Request, env: Env) -> Result<Response> {
    console_log!("Handling transcription request");

    // 1. Get the API Key from secrets
    let api_key = env.secret("OPENAI_API_KEY")?;
    console_log!("API Key retrieved.");

    // 2. Parse the incoming form data from the user's request
    let form_data = match req.form_data().await {
        Ok(data) => data,
        Err(e) => {
            console_log!("Failed to parse FormData: {:?}", e);
            return Response::error(format!("Failed to parse FormData: {}", e), 400);
        }
    };

    let file_entry = match form_data.get("file") {
        Some(entry) => entry,
        None => return Response::error("Missing 'file' field in FormData", 400),
    };

    let file = match file_entry {
        FormEntry::File(f) => f,
        _ => return Response::error("'file' field is not a file", 400),
    };

    console_log!("Received file: {}, size: {}", file.name(), file.size());

    // 3. Reconstruct the file to send to OpenAI
    let file_bytes = file.bytes().await?;

    let openai_form = web_sys::FormData::new().unwrap();

    let mut blob_options = web_sys::BlobPropertyBag::new();
    blob_options.type_(&file.type_());
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(
        &js_sys::Array::of1(&js_sys::Uint8Array::from(file_bytes.as_slice())),
        &blob_options,
    )
    .unwrap();

    openai_form
        .append_with_blob_and_filename("file", &blob, &file.name())
        .unwrap();
    openai_form.append_with_str("model", "whisper-1").unwrap();

    // 4. Create and send the request to OpenAI using the native Fetch API
    let mut headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {}", api_key.to_string()))?;

    let mut request_init = RequestInit::new();
    request_init
        .with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(openai_form.into()));

    let openai_req =
        Request::new_with_init("https://api.openai.com/v1/audio/transcriptions", &request_init)?;

    let mut openai_response = match Fetch::Request(openai_req).send().await {
        Ok(res) => res,
        Err(e) => {
            console_error!("OpenAI API request failed: {:?}", e);
            return Response::error(format!("OpenAI API request failed: {}", e), 500);
        }
    };

    // 5. Robustly handle the response from OpenAI
    let status = openai_response.status_code();
    let is_success = status >= 200 && status <= 299;

    let body_text = match openai_response.text().await {
        Ok(text) => text,
        Err(e) => {
            console_error!("Failed to read OpenAI response body: {:?}", e);
            return Response::error(format!("Failed to read OpenAI response body: {}", e), status);
        }
    };

    if !is_success {
        let error_body: OpenAIErrorResponse = match serde_json::from_str(&body_text) {
            Ok(body) => body,
            Err(e) => {
                console_error!(
                    "Failed to parse OpenAI error JSON: {:?}. Body: {}",
                    e,
                    body_text
                );
                return Response::error(
                    format!(
                        "OpenAI API error ({}) and failed to parse error details",
                        status
                    ),
                    status,
                );
            }
        };
        console_error!("OpenAI API error: {:?}", error_body);
        return Response::error(
            format!("OpenAI API Error: {}", error_body.error.message),
            status,
        );
    }

    match serde_json::from_str::<TranscriptionResponse>(&body_text) {
        Ok(transcription) => Response::ok(transcription.text),
        Err(e) => {
            console_error!(
                "Failed to parse OpenAI success JSON: {:?}. Body: {}",
                e,
                body_text
            );
            Response::error(format!("Failed to parse OpenAI API response: {}", e), 500)
        }
    }
}
