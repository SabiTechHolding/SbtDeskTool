use std::{
    io::{self, Read},
    time::Duration,
};

const KEYRING_SERVICE: &str = "com.sabitech.sbtdesktool.translation";

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let action = arguments
        .next()
        .ok_or("Usage: provider_credential <set|check|test> <provider-id>")?;
    let provider = arguments
        .next()
        .ok_or("Usage: provider_credential <set|check|test> <provider-id>")?;
    if arguments.next().is_some() {
        return Err("Too many arguments".into());
    }
    let entry =
        keyring::Entry::new(KEYRING_SERVICE, &provider).map_err(|error| error.to_string())?;
    match action.as_str() {
        "set" => {
            let mut secret = String::new();
            io::stdin()
                .read_to_string(&mut secret)
                .map_err(|error| error.to_string())?;
            if secret.trim().is_empty() {
                return Err("Provider API key is required on stdin".into());
            }
            entry
                .set_password(secret.trim())
                .map_err(|error| error.to_string())?;
            println!("{provider} credential stored");
            Ok(())
        }
        "check" => {
            entry.get_password().map_err(|error| error.to_string())?;
            println!("{provider} credential is available");
            Ok(())
        }
        "test" if provider == "gemini" => {
            let key = entry.get_password().map_err(|error| error.to_string())?;
            test_gemini(&key).await
        }
        "test" => Err(format!("Live test is not implemented for {provider}")),
        _ => Err("Unknown action. Use set, check, or test.".into()),
    }
}

async fn test_gemini(key: &str) -> Result<(), String> {
    let endpoint =
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.6-flash:generateContent";
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?
        .post(endpoint)
        .header("x-goog-api-key", key)
        .json(&serde_json::json!({
            "contents": [{
                "parts": [{
                    "text": "Translate 'Hello' from English to Vietnamese. Return only the translation."
                }]
            }]
        }))
        .send()
        .await
        .map_err(|error| format!("Gemini request failed: {error}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("Gemini returned invalid JSON: {error}"))?;
    if !status.is_success() {
        let message = body
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("request rejected");
        return Err(format!("Gemini HTTP {status}: {message}"));
    }
    let translated = body
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim();
    if translated.is_empty() {
        return Err("Gemini returned no translation".into());
    }
    println!("Gemini connection succeeded");
    Ok(())
}
