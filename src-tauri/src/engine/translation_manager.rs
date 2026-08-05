use crate::{
    commands::providers::{ProviderConnection, TranslationPolicy},
    engine::{translation_memory, translator},
};
use std::{
    collections::HashMap,
    process::Stdio,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
struct CacheEntry {
    result: translator::TranslateResult,
    stored_at: Instant,
}

static CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();

/// Shared policy entry point for text and file translation.
/// Dictionary and provider routing will be added here, so callers never need
/// to know whether their result came from memory or a remote provider.
pub async fn translate_with_fallback(
    text: &str,
    src: &str,
    dest: &str,
    providers: Vec<ProviderConnection>,
    strategy: u8,
    policy: TranslationPolicy,
) -> Result<translator::TranslateResult, String> {
    if let Some(result) = lookup_local(text, src, dest, strategy, &policy)? {
        return Ok(result);
    }

    let (remote_text, protected_terms) = if policy.use_dictionary {
        protect_dictionary_terms(text, translation_memory::dictionary_terms(text, src, dest)?)
    } else {
        (text.to_string(), Vec::new())
    };
    let mut failures = Vec::new();
    let mut result = None;
    for provider in providers {
        match translate_remote(&remote_text, src, dest, &provider, strategy).await {
            Ok(mut value) => {
                restore_dictionary_terms(&mut value.translated, &protected_terms);
                result = Some(value);
                break;
            }
            Err(error) => failures.push(format!("{}: {error}", provider.name)),
        }
    }
    let result = result.ok_or_else(|| {
        if failures.is_empty() {
            "No translation provider is available".to_string()
        } else {
            format!("All translation providers failed: {}", failures.join("; "))
        }
    })?;
    persist_result(text, src, dest, &result, &policy)?;
    Ok(result)
}

pub async fn translate_many_with_fallback(
    texts: &[String],
    src: &str,
    dest: &str,
    providers: Vec<ProviderConnection>,
    strategy: u8,
    policy: TranslationPolicy,
) -> Result<Vec<Result<translator::TranslateResult, String>>, String> {
    if texts.len() >= 2
        && providers
            .first()
            .is_some_and(|provider| supports_prompt_batch(&provider.id))
    {
        return translate_prompt_batch_with_fallback(texts, src, dest, providers, strategy, policy)
            .await;
    }
    if providers.first().map(|provider| provider.id.as_str()) != Some("google") || texts.len() < 2 {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(
                translate_with_fallback(
                    text,
                    src,
                    dest,
                    providers.clone(),
                    strategy,
                    policy.clone(),
                )
                .await,
            );
        }
        return Ok(results);
    }

    let mut results = vec![None; texts.len()];
    let mut remote_texts = Vec::new();
    let mut remote_positions = Vec::new();
    let mut protected_terms = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        if let Some(result) = lookup_local(text, src, dest, strategy, &policy)? {
            results[index] = Some(Ok(result));
        } else {
            let (remote_text, replacements) = if policy.use_dictionary {
                protect_dictionary_terms(
                    text,
                    translation_memory::dictionary_terms(text, src, dest)?,
                )
            } else {
                (text.clone(), Vec::new())
            };
            remote_texts.push(remote_text);
            remote_positions.push(index);
            protected_terms.push(replacements);
        }
    }

    if !remote_texts.is_empty() {
        let batch_results =
            translate_google_batch(&remote_texts, src, dest, &providers[0], strategy).await;
        for (remote_index, batch_result) in batch_results.into_iter().enumerate() {
            let original_index = remote_positions[remote_index];
            let result = match batch_result {
                Ok(mut result) => {
                    restore_dictionary_terms(
                        &mut result.translated,
                        &protected_terms[remote_index],
                    );
                    persist_result(&texts[original_index], src, dest, &result, &policy)?;
                    Ok(result)
                }
                Err(_) => {
                    translate_with_fallback(
                        &texts[original_index],
                        src,
                        dest,
                        providers.clone(),
                        strategy,
                        policy.clone(),
                    )
                    .await
                }
            };
            results[original_index] = Some(result);
        }
    }

    Ok(results
        .into_iter()
        .map(|result| result.unwrap_or_else(|| Err("Translation result is missing".into())))
        .collect())
}

fn supports_prompt_batch(provider_id: &str) -> bool {
    matches!(
        provider_id,
        "gemini" | "openai" | "claude" | "local" | "agent_cli"
    )
}

async fn translate_prompt_batch_with_fallback(
    texts: &[String],
    src: &str,
    dest: &str,
    providers: Vec<ProviderConnection>,
    strategy: u8,
    policy: TranslationPolicy,
) -> Result<Vec<Result<translator::TranslateResult, String>>, String> {
    let mut results = vec![None; texts.len()];
    let mut remote_texts = Vec::new();
    let mut remote_positions = Vec::new();
    let mut protected_terms = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        if let Some(result) = lookup_local(text, src, dest, strategy, &policy)? {
            results[index] = Some(Ok(result));
        } else {
            let (remote_text, replacements) = if policy.use_dictionary {
                protect_dictionary_terms(
                    text,
                    translation_memory::dictionary_terms(text, src, dest)?,
                )
            } else {
                (text.clone(), Vec::new())
            };
            remote_texts.push(remote_text);
            remote_positions.push(index);
            protected_terms.push(replacements);
        }
    }
    if remote_texts.is_empty() {
        return Ok(results
            .into_iter()
            .map(|result| result.expect("all local batch results are populated"))
            .collect());
    }

    let mut failures = Vec::new();
    let mut translated_batch = None;
    for provider in &providers {
        let batch_result = if supports_prompt_batch(&provider.id) {
            translate_prompt_batch_remote(&remote_texts, src, dest, provider).await
        } else if provider.id == "google" {
            let values = translate_google_batch(&remote_texts, src, dest, provider, strategy).await;
            values.into_iter().collect::<Result<Vec<_>, _>>()
        } else {
            Err(format!(
                "{} does not support batch translation",
                provider.name
            ))
        };
        match batch_result {
            Ok(values) => {
                translated_batch = Some(values);
                break;
            }
            Err(error) => failures.push(format!(
                "{}: {}",
                provider.name,
                redact_provider_error(&error, provider)
            )),
        }
    }
    let translated_batch = translated_batch
        .ok_or_else(|| format!("All translation providers failed: {}", failures.join("; ")))?;
    if translated_batch.len() != remote_texts.len() {
        return Err("Batch provider returned an unexpected number of translations".into());
    }
    for (remote_index, mut value) in translated_batch.into_iter().enumerate() {
        restore_dictionary_terms(&mut value.translated, &protected_terms[remote_index]);
        let original_index = remote_positions[remote_index];
        persist_result(&texts[original_index], src, dest, &value, &policy)?;
        results[original_index] = Some(Ok(value));
    }
    Ok(results
        .into_iter()
        .map(|result| result.unwrap_or_else(|| Err("Translation result is missing".into())))
        .collect())
}

fn lookup_local(
    text: &str,
    src: &str,
    dest: &str,
    strategy: u8,
    policy: &TranslationPolicy,
) -> Result<Option<translator::TranslateResult>, String> {
    if policy.use_dictionary {
        if let Some(hit) = translation_memory::lookup_dictionary(text, src, dest)? {
            return Ok(Some(translator::TranslateResult {
                translated: hit.translated,
                detected_lang: Some(src.to_string()),
                source: hit.source,
                strategy,
            }));
        }
    }
    if policy.use_translation_memory {
        if let Some(hit) = translation_memory::lookup(text, src, dest)? {
            return Ok(Some(translator::TranslateResult {
                translated: hit.translated,
                detected_lang: Some(src.to_string()),
                source: format!("Translation Memory ({})", hit.source),
                strategy,
            }));
        }
    }
    if policy.use_cache {
        let key = cache_key(text, src, dest);
        if let Some(hit) = CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?
            .get(&key)
            .cloned()
            .filter(|entry| {
                entry.stored_at.elapsed() < Duration::from_secs(policy.cache_ttl_seconds.max(1))
            })
        {
            return Ok(Some(translator::TranslateResult {
                source: format!("Cache ({})", hit.result.source),
                ..hit.result
            }));
        }
    }
    Ok(None)
}

fn persist_result(
    text: &str,
    src: &str,
    dest: &str,
    result: &translator::TranslateResult,
    policy: &TranslationPolicy,
) -> Result<(), String> {
    if policy.use_cache {
        CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?
            .insert(
                cache_key(text, src, dest),
                CacheEntry {
                    result: result.clone(),
                    stored_at: Instant::now(),
                },
            );
    }
    if policy.save_translation_memory {
        translation_memory::store(text, &result.translated, src, dest, &result.source)?;
    }
    Ok(())
}

fn cache_key(text: &str, src: &str, dest: &str) -> String {
    format!("{src}\u{1f}{dest}\u{1f}{text}")
}

async fn translate_google_batch(
    texts: &[String],
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
    strategy: u8,
) -> Vec<Result<translator::TranslateResult, String>> {
    let mut last_error = format!("{} batch failed", provider.name);
    for attempt in 0..=provider.retries {
        match tokio::time::timeout(
            Duration::from_secs(provider.timeout_seconds.max(1)),
            translator::translate_batch(texts, src, dest, strategy),
        )
        .await
        {
            Ok(results) if results.iter().any(Result::is_ok) => return results,
            Ok(results) => {
                last_error = results
                    .into_iter()
                    .find_map(Result::err)
                    .unwrap_or_else(|| format!("{} batch returned no results", provider.name));
            }
            Err(_) => {
                last_error = format!(
                    "{} batch timed out after {} seconds",
                    provider.name, provider.timeout_seconds
                );
            }
        }
        if attempt < provider.retries {
            let delay = 250u64.saturating_mul(1u64 << attempt.min(4));
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }
    vec![Err(last_error); texts.len()]
}

async fn translate_remote(
    text: &str,
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
    strategy: u8,
) -> Result<translator::TranslateResult, String> {
    let mut last_error = String::new();
    for attempt in 0..=provider.retries {
        let result = tokio::time::timeout(
            Duration::from_secs(provider.timeout_seconds.max(1)),
            translate_remote_once(text, src, dest, provider, strategy),
        )
        .await;
        match result {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => last_error = redact_provider_error(&error, provider),
            Err(_) => {
                last_error = format!(
                    "{} timed out after {} seconds",
                    provider.name, provider.timeout_seconds
                )
            }
        }
        if attempt < provider.retries {
            tokio::time::sleep(retry_delay(&last_error, attempt)).await;
        }
    }
    Err(last_error)
}

fn redact_provider_error(error: &str, provider: &ProviderConnection) -> String {
    provider
        .api_key
        .as_deref()
        .filter(|key| !key.is_empty())
        .map(|key| error.replace(key, "[REDACTED]"))
        .unwrap_or_else(|| error.to_string())
}

fn retry_delay(error: &str, attempt: u32) -> Duration {
    let base_millis: u64 = if error.contains("HTTP 429") {
        2_000
    } else {
        250
    };
    Duration::from_millis(base_millis.saturating_mul(1u64 << attempt.min(4)))
}

async fn translate_prompt_batch_remote(
    texts: &[String],
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
) -> Result<Vec<translator::TranslateResult>, String> {
    let prompt = batch_translation_prompt(texts, src, dest)?;
    let mut last_error = String::new();
    for attempt in 0..=provider.retries {
        let result = tokio::time::timeout(
            Duration::from_secs(provider.timeout_seconds.max(1)),
            call_prompt_provider(&prompt, provider),
        )
        .await;
        match result {
            Ok(Ok(response)) => match parse_batch_translations(&response, texts.len(), provider) {
                Ok(values) => return Ok(values),
                Err(error) => last_error = error,
            },
            Ok(Err(error)) => last_error = redact_provider_error(&error, provider),
            Err(_) => {
                last_error = format!(
                    "{} batch timed out after {} seconds",
                    provider.name, provider.timeout_seconds
                )
            }
        }
        if attempt < provider.retries {
            tokio::time::sleep(retry_delay(&last_error, attempt)).await;
        }
    }
    Err(last_error)
}

fn batch_translation_prompt(texts: &[String], src: &str, dest: &str) -> Result<String, String> {
    let items = texts
        .iter()
        .enumerate()
        .map(|(id, text)| serde_json::json!({"id": id, "text": text}))
        .collect::<Vec<_>>();
    let serialized = serde_json::to_string(&items).map_err(|error| error.to_string())?;
    Ok(format!(
        "Translate every item from {src} to {dest}. Preserve line breaks and placeholders such as __SBT_TERM_0__ exactly. Return ONLY a JSON array with one object per input: {{\"id\": number, \"translation\": string}}. Keep every id exactly once and do not add commentary.\n\nINPUT:\n{serialized}"
    ))
}

#[derive(serde::Deserialize)]
struct BatchTranslationItem {
    id: usize,
    translation: String,
}

fn parse_batch_translations(
    response: &str,
    expected: usize,
    provider: &ProviderConnection,
) -> Result<Vec<translator::TranslateResult>, String> {
    let start = response
        .find('[')
        .ok_or_else(|| format!("{} batch returned no JSON array", provider.name))?;
    let end = response
        .rfind(']')
        .ok_or_else(|| format!("{} batch returned incomplete JSON", provider.name))?;
    let items: Vec<BatchTranslationItem> = serde_json::from_str(&response[start..=end])
        .map_err(|error| format!("{} batch returned invalid JSON: {error}", provider.name))?;
    let mut ordered = vec![None; expected];
    for item in items {
        if item.id >= expected || ordered[item.id].is_some() || item.translation.trim().is_empty() {
            return Err(format!(
                "{} batch returned invalid or duplicate id {}",
                provider.name, item.id
            ));
        }
        ordered[item.id] = Some(item.translation.trim().to_string());
    }
    if ordered.iter().any(Option::is_none) {
        return Err(format!(
            "{} batch omitted one or more translations",
            provider.name
        ));
    }
    Ok(ordered
        .into_iter()
        .map(|translation| translator::TranslateResult {
            translated: translation.expect("validated batch translation"),
            detected_lang: None,
            source: provider.name.clone(),
            strategy: 0,
        })
        .collect())
}

async fn call_prompt_provider(
    prompt: &str,
    provider: &ProviderConnection,
) -> Result<String, String> {
    match provider.id.as_str() {
        "local" | "openai" => call_openai_prompt(prompt, provider).await,
        "gemini" => call_gemini_prompt(prompt, provider).await,
        "claude" => call_claude_prompt(prompt, provider).await,
        "agent_cli" => call_agent_cli(prompt, provider).await,
        _ => Err(format!("{} does not support prompt batches", provider.name)),
    }
}

async fn translate_remote_once(
    text: &str,
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
    strategy: u8,
) -> Result<translator::TranslateResult, String> {
    let result = match provider.id.as_str() {
        "google" => translator::translate(text, src, dest, strategy).await?,
        "local" | "openai" => translate_openai_compatible(text, src, dest, provider).await?,
        "gemini" => translate_gemini(text, src, dest, provider).await?,
        "claude" => translate_claude(text, src, dest, provider).await?,
        "deepl" => translate_deepl(text, src, dest, provider).await?,
        "agent_cli" => translate_agent_cli(text, src, dest, provider).await?,
        _ => return Err(format!("{} is not implemented yet.", provider.name)),
    };
    Ok(result)
}

pub async fn test_connection(provider: ProviderConnection, strategy: u8) -> Result<String, String> {
    let result = translate_remote("Hello", "en", "vi", &provider, strategy).await?;
    Ok(format!("{} connection succeeded", result.source))
}

fn translation_prompt(text: &str, src: &str, dest: &str) -> String {
    format!("Translate the following text from {src} to {dest}. Return only the translation; preserve line breaks and placeholders such as __SBT_TERM_0__ exactly.\n\n{text}")
}

async fn translate_agent_cli(
    text: &str,
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
) -> Result<translator::TranslateResult, String> {
    let translated = call_agent_cli(&translation_prompt(text, src, dest), provider).await?;
    Ok(translator::TranslateResult {
        translated,
        detected_lang: (src != "auto").then(|| src.to_string()),
        source: provider.name.clone(),
        strategy: 0,
    })
}

fn agent_cli_arguments(arguments: &str, prompt: &str) -> (Vec<String>, bool) {
    let mut uses_prompt_argument = false;
    let arguments = arguments
        .lines()
        .map(str::trim)
        .filter(|argument| !argument.is_empty())
        .map(|argument| {
            if argument.contains("{prompt}") {
                uses_prompt_argument = true;
                argument.replace("{prompt}", prompt)
            } else {
                argument.to_string()
            }
        })
        .collect();
    (arguments, uses_prompt_argument)
}

async fn call_agent_cli(prompt: &str, provider: &ProviderConnection) -> Result<String, String> {
    let executable = provider.model.trim();
    if executable.is_empty() {
        return Err("Agent CLI executable is missing".into());
    }
    let (arguments, uses_prompt_argument) = agent_cli_arguments(&provider.base_url, prompt);
    let mut command = tokio::process::Command::new(executable);
    command
        .args(arguments)
        .stdin(if uses_prompt_argument {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Unable to start Agent CLI executable '{executable}': {error}"))?;

    if !uses_prompt_argument {
        let mut stdin = child.stdin.take().ok_or("Unable to open Agent CLI stdin")?;
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|error| format!("Unable to write Agent CLI prompt: {error}"))?;
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|error| format!("Unable to wait for Agent CLI: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        let detail = if stderr.is_empty() { &stdout } else { &stderr };
        return Err(if detail.is_empty() {
            format!("Agent CLI exited with {}", output.status)
        } else {
            format!("Agent CLI exited with {}: {detail}", output.status)
        });
    }
    if stdout.is_empty() {
        return Err(if stderr.is_empty() {
            "Agent CLI returned no translation".into()
        } else {
            format!("Agent CLI returned no translation: {stderr}")
        });
    }
    Ok(stdout)
}

fn protect_dictionary_terms(
    text: &str,
    terms: Vec<(String, String)>,
) -> (String, Vec<(String, String)>) {
    let mut protected = text.to_string();
    let mut replacements = Vec::new();
    for (source, translation) in terms {
        if !protected.contains(&source) {
            continue;
        }
        let placeholder = format!("__SBT_TERM_{}__", replacements.len());
        protected = protected.replace(&source, &placeholder);
        replacements.push((placeholder, translation));
    }
    (protected, replacements)
}

fn restore_dictionary_terms(text: &mut String, replacements: &[(String, String)]) {
    for (placeholder, translation) in replacements {
        *text = text.replace(placeholder, translation);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        agent_cli_arguments, parse_batch_translations, protect_dictionary_terms,
        redact_provider_error, restore_dictionary_terms, retry_delay, test_connection,
        translate_many_with_fallback,
    };

    #[test]
    fn agent_cli_supports_prompt_argument_or_stdin() {
        let (arguments, uses_prompt_argument) =
            agent_cli_arguments("exec\n--prompt\n{prompt}\n--quiet", "Hello world");
        assert_eq!(
            arguments,
            vec!["exec", "--prompt", "Hello world", "--quiet"]
        );
        assert!(uses_prompt_argument);

        let (arguments, uses_prompt_argument) = agent_cli_arguments("exec\n-", "ignored");
        assert_eq!(arguments, vec!["exec", "-"]);
        assert!(!uses_prompt_argument);
    }
    use crate::commands::providers::{ProviderConnection, TranslationPolicy};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::Duration,
    };

    #[test]
    fn protects_dictionary_terms_inside_longer_text() {
        let (masked, replacements) =
            protect_dictionary_terms("保存ボタンを押す", vec![("保存".into(), "Lưu".into())]);
        assert_eq!(masked, "__SBT_TERM_0__ボタンを押す");
        let mut translated = "Nhấn nút __SBT_TERM_0__".to_string();
        restore_dictionary_terms(&mut translated, &replacements);
        assert_eq!(translated, "Nhấn nút Lưu");
    }

    #[test]
    fn redacts_provider_api_key_from_errors() {
        let provider = ProviderConnection {
            id: "gemini".into(),
            name: "Gemini".into(),
            model: "gemini-3.6-flash".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api_key: Some("secret-test-key".into()),
            timeout_seconds: 30,
            retries: 1,
            concurrency: 1,
        };
        let error = "request failed for https://example.test?key=secret-test-key";
        let redacted = redact_provider_error(error, &provider);
        assert!(!redacted.contains("secret-test-key"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn rate_limit_errors_use_longer_backoff() {
        assert_eq!(
            retry_delay("Gemini HTTP 429 Too Many Requests", 0),
            Duration::from_secs(2)
        );
        assert_eq!(
            retry_delay("Gemini HTTP 429 Too Many Requests", 1),
            Duration::from_secs(4)
        );
        assert_eq!(
            retry_delay("temporary request error", 0),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn parses_prompt_batch_by_id_instead_of_response_order() {
        let provider = ProviderConnection {
            id: "gemini".into(),
            name: "Gemini".into(),
            model: "gemini-3.6-flash".into(),
            base_url: String::new(),
            api_key: None,
            timeout_seconds: 30,
            retries: 1,
            concurrency: 1,
        };
        let response =
            "```json\n[{\"id\":1,\"translation\":\"Hai\"},{\"id\":0,\"translation\":\"Một\"}]\n```";
        let values = parse_batch_translations(response, 2, &provider).expect("valid batch");
        assert_eq!(values[0].translated, "Một");
        assert_eq!(values[1].translated, "Hai");
    }

    #[tokio::test]
    async fn sends_multiple_ai_items_in_one_prompt_request() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
        let address = listener.local_addr().expect("mock address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept provider request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = stream.read(&mut buffer).expect("read provider request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|value| value == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request);
            assert!(request.contains("\\\"id\\\":0"));
            assert!(request.contains("\\\"id\\\":1"));
            let content = r#"[{"id":1,"translation":"Hủy"},{"id":0,"translation":"Lưu"}]"#;
            let body = serde_json::json!({
                "choices": [{"message": {"content": content}}]
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write provider response");
        });
        let provider = ProviderConnection {
            id: "local".into(),
            name: "Mock AI".into(),
            model: "mock".into(),
            base_url: format!("http://{address}/v1"),
            api_key: None,
            timeout_seconds: 5,
            retries: 0,
            concurrency: 1,
        };
        let results = translate_many_with_fallback(
            &["保存".into(), "キャンセル".into()],
            "ja",
            "vi",
            vec![provider],
            0,
            TranslationPolicy {
                use_dictionary: false,
                use_translation_memory: false,
                use_cache: false,
                save_translation_memory: false,
                cache_ttl_seconds: 60,
            },
        )
        .await
        .expect("batch translation");
        server.join().expect("mock provider");
        assert_eq!(results[0].as_ref().expect("first item").translated, "Lưu");
        assert_eq!(results[1].as_ref().expect("second item").translated, "Hủy");
    }

    #[tokio::test]
    #[ignore = "requires a Gemini API key in Windows Credential Manager"]
    async fn live_gemini_smoke() {
        let api_key = keyring::Entry::new("com.sabitech.sbtdesktool.translation", "gemini")
            .expect("Gemini credential entry")
            .get_password()
            .expect("stored Gemini API key");
        let provider = ProviderConnection {
            id: "gemini".into(),
            name: "Gemini".into(),
            model: "gemini-3.6-flash".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api_key: Some(api_key),
            timeout_seconds: 30,
            retries: 1,
            concurrency: 1,
        };
        assert!(test_connection(provider, 0)
            .await
            .expect("Gemini translation request")
            .contains("succeeded"));
    }
}

async fn translate_openai_compatible(
    text: &str,
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
) -> Result<translator::TranslateResult, String> {
    let translated = call_openai_prompt(&translation_prompt(text, src, dest), provider).await?;
    Ok(translator::TranslateResult {
        translated,
        detected_lang: Some(src.into()),
        source: provider.name.clone(),
        strategy: 0,
    })
}

async fn call_openai_prompt(prompt: &str, provider: &ProviderConnection) -> Result<String, String> {
    let endpoint = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let mut request = reqwest::Client::builder().timeout(Duration::from_secs(provider.timeout_seconds.max(1))).build().map_err(|error| error.to_string())?
        .post(endpoint).json(&serde_json::json!({"model": provider.model, "messages": [{"role": "user", "content": prompt}], "temperature": 0}));
    if let Some(api_key) = &provider.api_key {
        request = request.bearer_auth(api_key);
    }
    let body: serde_json::Value = request
        .send()
        .await
        .map_err(|error| format!("{} request failed: {error}", provider.name))?
        .error_for_status()
        .map_err(|error| format!("{} HTTP error: {error}", provider.name))?
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let translated = body
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if translated.is_empty() {
        return Err(format!("{} returned no translation", provider.name));
    }
    Ok(translated)
}

async fn translate_gemini(
    text: &str,
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
) -> Result<translator::TranslateResult, String> {
    let translated = call_gemini_prompt(&translation_prompt(text, src, dest), provider).await?;
    Ok(translator::TranslateResult {
        translated,
        detected_lang: Some(src.into()),
        source: provider.name.clone(),
        strategy: 0,
    })
}

async fn call_gemini_prompt(prompt: &str, provider: &ProviderConnection) -> Result<String, String> {
    let key = provider
        .api_key
        .as_deref()
        .ok_or("Gemini API key is missing")?;
    let endpoint = format!(
        "{}/models/{}:generateContent",
        provider.base_url.trim_end_matches('/'),
        provider.model
    );
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(provider.timeout_seconds.max(1)))
        .build()
        .map_err(|error| error.to_string())?
        .post(endpoint)
        .header("x-goog-api-key", key)
        .json(&serde_json::json!({
            "contents": [{"parts": [{"text": prompt}]}]
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
    body.pointer("/candidates/0/content/parts/0/text")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Gemini returned no translation".into())
}

async fn translate_claude(
    text: &str,
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
) -> Result<translator::TranslateResult, String> {
    let translated = call_claude_prompt(&translation_prompt(text, src, dest), provider).await?;
    Ok(translator::TranslateResult {
        translated,
        detected_lang: Some(src.into()),
        source: provider.name.clone(),
        strategy: 0,
    })
}

async fn call_claude_prompt(prompt: &str, provider: &ProviderConnection) -> Result<String, String> {
    let key = provider
        .api_key
        .as_deref()
        .ok_or("Claude API key is missing")?;
    let endpoint = format!("{}/v1/messages", provider.base_url.trim_end_matches('/'));
    let body: serde_json::Value = reqwest::Client::builder().timeout(Duration::from_secs(provider.timeout_seconds.max(1))).build().map_err(|e| e.to_string())?.post(endpoint).header("x-api-key",key).header("anthropic-version","2023-06-01").json(&serde_json::json!({"model":provider.model,"max_tokens":4096,"temperature":0,"messages":[{"role":"user","content":prompt}]})).send().await.map_err(|e| format!("Claude request failed: {e}"))?.error_for_status().map_err(|e| format!("Claude HTTP error: {e}"))?.json().await.map_err(|e| e.to_string())?;
    body.pointer("/content/0/text")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Claude returned no translation".into())
}

async fn translate_deepl(
    text: &str,
    src: &str,
    dest: &str,
    provider: &ProviderConnection,
) -> Result<translator::TranslateResult, String> {
    let key = provider
        .api_key
        .as_deref()
        .ok_or("DeepL API key is missing")?;
    let endpoint = format!("{}/v2/translate", provider.base_url.trim_end_matches('/'));
    let body: serde_json::Value = reqwest::Client::builder()
        .timeout(Duration::from_secs(provider.timeout_seconds.max(1)))
        .build()
        .map_err(|error| error.to_string())?
        .post(endpoint)
        .header("Authorization", format!("DeepL-Auth-Key {key}"))
        .form(&[
            ("text", text),
            ("source_lang", &src.to_uppercase()),
            ("target_lang", &dest.to_uppercase()),
        ])
        .send()
        .await
        .map_err(|e| format!("DeepL request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("DeepL HTTP error: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    provider_result(
        body.pointer("/translations/0/text")
            .and_then(|v| v.as_str()),
        src,
        provider,
    )
}

fn provider_result(
    value: Option<&str>,
    src: &str,
    provider: &ProviderConnection,
) -> Result<translator::TranslateResult, String> {
    let translated = value.unwrap_or_default().trim();
    if translated.is_empty() {
        return Err(format!("{} returned no translation", provider.name));
    }
    Ok(translator::TranslateResult {
        translated: translated.into(),
        detected_lang: Some(src.into()),
        source: provider.name.clone(),
        strategy: 0,
    })
}
