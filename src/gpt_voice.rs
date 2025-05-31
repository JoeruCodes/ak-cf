use reqwest::multipart;
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

    let api_key = env.secret("OPENAI_API_KEY")?;

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
    let file_bytes = file.bytes().await?;
    let file_part = multipart::Part::bytes(file_bytes)
        .file_name(file.name())
        .mime_str(file.type_().as_str())
        .map_err(|e| worker::Error::RustError(format!("Failed to create file part: {}", e)))?; // Use the mime type from the uploaded file

    let model_part = multipart::Part::text("whisper-1");

    let form = multipart::Form::new()
        .part("file", file_part)
        .part("model", model_part);

    let client = reqwest::Client::new();
    let openai_response = match client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => {
            console_error!("OpenAI API request failed: {:?}", e);
            return Response::error(format!("OpenAI API request failed: {}", e), 500);
        }
    };

    if !openai_response.status().is_success() {
        let status = openai_response.status();
        let error_body: OpenAIErrorResponse = match openai_response.json().await {
            Ok(body) => body,
            Err(e) => {
                console_error!("Failed to parse OpenAI error response: {:?}", e);
                return Response::error(
                    format!(
                        "OpenAI API error ({}) and failed to parse error details",
                        status
                    ),
                    status.as_u16(),
                );
            }
        };
        console_error!("OpenAI API error: {:?}", error_body);
        return Response::error(
            format!("OpenAI API Error: {}", error_body.error.message),
            status.as_u16(),
        );
    }

    match openai_response.json::<TranscriptionResponse>().await {
        Ok(transcription) => Response::ok(transcription.text),
        Err(e) => {
            console_error!("Failed to parse OpenAI success response: {:?}", e);
            Response::error(format!("Failed to parse OpenAI API response: {}", e), 500)
        }
    }
}
