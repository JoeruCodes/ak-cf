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

