use serde::{Deserialize, Serialize};
use worker::*;


#[derive(Serialize, Deserialize, Debug)]
pub struct ClientSecret {
    pub value: String,
    pub expires_at: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GptVoiceKeyResponse {
    pub client_secret: ClientSecret,
}

// Add this struct, for example, after your RegisterBody struct
#[derive(Deserialize, Serialize, Debug)] // Added Serialize for completeness, Debug for logging
struct OpenAiTranscriptionResponse {
    text: String,
}


pub async fn fetch_gpt_voice_key(_: &Env) -> Result<String> {
    let api_url = ""; // Replace with the actual endpoint

    let mut req = Request::new(api_url, Method::Post)?;

    // Hardcoded API key directly in code
    let hardcoded_key = "";

    req.headers_mut()?.set("Authorization", &format!("Bearer {}", hardcoded_key))?;
    req.headers_mut()?.set("Content-Type", "application/json")?;

    let mut res = Fetch::Request(req).send().await?;


    println!("{}",res.status_code());
    let status = res.status_code();
    if status < 200 || status >= 300 {
        return Err(Error::RustError(format!(
            "GPT Voice token fetch failed: {}",
            status
        )));
    }


   let gpt_key_response: GptVoiceKeyResponse = res.json().await?;
let secret_value = gpt_key_response.client_secret.value;

println!("Client secret key: {}", secret_value);
Ok(secret_value)
}


// Add this new async function
// pub async fn handle_transcription(mut req: Request, env: Env) -> Result<Response> {
//     console_log!("Handling transcription request...");

//     let openai_api_key = env.secret("OPENAI_API_KEY")
//         .map_err(|e| {
//             console_error!("OPENAI_API_KEY not found: {:?}", e);
//             Error::RustError("OpenAI API key not configured".into())
//         })?
//         .to_string();

//     // Read the audio bytes
//     let audio_bytes = req.bytes().await.map_err(|e| {
//         console_error!("Failed to read request body: {:?}", e);
//         Error::RustError("Failed to read audio data".into())
//     })?;

//     if audio_bytes.is_empty() {
//         return Response::error("No audio data received", 400);
//     }

//     // Create the audio file and set MIME type
//     let mut audio_file = File::new("audio.wav", audio_bytes);
//     audio_file.set_type("audio/wav");

//     // Prepare multipart form-data
//     let mut form_data = FormData::new();
//     form_data.append("file", FormEntry::File(audio_file))?;
//     form_data.append("model", FormEntry::Field("gpt-4o-transcribe".to_string()))?;

//     // Set headers
//     let mut headers = Headers::new();
//     headers.set("Authorization", &format!("Bearer {}", openai_api_key))?;

//     // Build OpenAI request
//     let mut request_init = RequestInit::new();
//     request_init.with_method(Method::Post);
//     request_init.with_body(Some(form_data.into()));
//     request_init.with_headers(headers);

//     let openai_request = Request::new_with_init(
//         "https://api.openai.com/v1/audio/transcriptions",
//         &request_init,
//     )?;

//     let mut openai_response = Fetch::Request(openai_request).send().await.map_err(|e| {
//         console_error!("OpenAI API request failed: {:?}", e);
//         Error::RustError("OpenAI API request failed".into())
//     })?;

//     if openai_response.status_code() != 200 {
//     let status = openai_response.status_code();
//     let err_msg = openai_response.text().await.unwrap_or_else(|_| "Failed to read OpenAI error response".to_string());
//     return Response::error(format!("OpenAI API error ({}): {}", status, err_msg), 500);
// }

//     let response_json: OpenAiTranscriptionResponse = openai_response.json().await.map_err(|e| {
//         console_error!("Failed to parse OpenAI response: {:?}", e);
//         Error::RustError("Failed to parse transcription from OpenAI".into())
//     })?;

//     Response::ok(response_json.text)
// }