use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TranslateResult {
    pub translated: String,
    pub detected_lang: Option<String>,
    pub source: String,
    pub strategy: u8,
}

const MAX_CHARS: usize = 700;
const MAX_BATCH_CHARS: usize = 640;
const MAX_BATCH_ITEMS: usize = 20;

pub async fn translate(
    text: &str,
    src: &str,
    dest: &str,
    strategy: u8,
) -> Result<TranslateResult, String> {
    // An empty/whitespace-only edit is a local no-op and must not produce a
    // network request.
    if text.trim().is_empty() {
        return Ok(TranslateResult {
            translated: String::new(),
            detected_lang: Some(src.to_string()),
            source: "Google Translate".into(),
            strategy,
        });
    }
    let chunks = split_text(text, MAX_CHARS);
    let mut translated = String::new();
    let mut detected_lang = None;
    let mut working_strategy = strategy;
    for chunk in chunks {
        let result = translate_single(&chunk, src, dest, working_strategy).await?;
        translated.push_str(&result.translated);
        working_strategy = result.strategy;
        if let Some(detected) = result.detected_lang {
            if detected != "auto" {
                detected_lang = Some(detected);
            }
        }
    }
    Ok(TranslateResult {
        translated,
        detected_lang,
        source: "Google Translate".into(),
        strategy: working_strategy,
    })
}

pub async fn translate_batch(
    texts: &[String],
    src: &str,
    dest: &str,
    strategy: u8,
) -> Vec<Result<TranslateResult, String>> {
    let mut results = vec![Err("Translation batch was not processed".into()); texts.len()];
    let mut working_strategy = strategy;
    let mut start = 0usize;
    while start < texts.len() {
        let end = batch_end(texts, start);
        let payload = build_batch_payload(texts, start, end);
        match translate(&payload, src, dest, working_strategy).await {
            Ok(batch_result) => {
                working_strategy = batch_result.strategy;
                match parse_batch_result(&batch_result.translated, start, end) {
                    Ok(values) => {
                        for (offset, translated) in values.into_iter().enumerate() {
                            results[start + offset] = Ok(TranslateResult {
                                translated,
                                detected_lang: batch_result.detected_lang.clone(),
                                source: batch_result.source.clone(),
                                strategy: batch_result.strategy,
                            });
                        }
                    }
                    Err(error) => {
                        for result in &mut results[start..end] {
                            *result = Err(error.clone());
                        }
                    }
                }
            }
            Err(error) => {
                for result in &mut results[start..end] {
                    *result = Err(error.clone());
                }
            }
        }
        start = end;
    }
    results
}

fn batch_marker(index: usize, boundary: &str) -> String {
    format!("__SBT_ITEM_{index:04}_{boundary}__")
}

fn batch_item_size(index: usize, text: &str) -> usize {
    batch_marker(index, "BEGIN").chars().count()
        + batch_marker(index, "END").chars().count()
        + text.chars().count()
        + 3
}

fn batch_end(texts: &[String], start: usize) -> usize {
    let mut end = start;
    let mut chars = 0usize;
    while end < texts.len() && end - start < MAX_BATCH_ITEMS {
        let next = batch_item_size(end, &texts[end]);
        if end > start && chars + next > MAX_BATCH_CHARS {
            break;
        }
        chars += next;
        end += 1;
    }
    end.max(start + 1)
}

fn build_batch_payload(texts: &[String], start: usize, end: usize) -> String {
    let mut payload = String::new();
    for (index, text) in texts.iter().enumerate().take(end).skip(start) {
        payload.push_str(&batch_marker(index, "BEGIN"));
        payload.push('\n');
        payload.push_str(text);
        payload.push('\n');
        payload.push_str(&batch_marker(index, "END"));
        payload.push('\n');
    }
    payload
}

fn parse_batch_result(translated: &str, start: usize, end: usize) -> Result<Vec<String>, String> {
    let mut values = Vec::with_capacity(end - start);
    for index in start..end {
        let begin = batch_marker(index, "BEGIN");
        let finish = batch_marker(index, "END");
        let after_begin = translated
            .find(&begin)
            .map(|position| position + begin.len())
            .ok_or_else(|| format!("Google batch response is missing marker {begin}"))?;
        let relative_end = translated[after_begin..]
            .find(&finish)
            .ok_or_else(|| format!("Google batch response is missing marker {finish}"))?;
        values.push(
            translated[after_begin..after_begin + relative_end]
                .trim_matches(['\r', '\n'])
                .to_string(),
        );
    }
    Ok(values)
}

async fn translate_single(
    text: &str,
    src: &str,
    dest: &str,
    strategy: u8,
) -> Result<TranslateResult, String> {
    let src_param = if src == "auto" { "auto" } else { src };
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t&q={}",
        src_param,
        dest,
        urlencoding(text)
    );

    let primary = crate::engine::network::request_with_strategies(&url, strategy).await;
    let (body, working_strategy) = match primary {
        Ok(response) => response,
        Err(primary_error) => {
            let fallback_url = url.replace("translate.googleapis.com", "translate.google.com");
            crate::engine::network::request_with_strategies(&fallback_url, strategy)
                .await
                .map_err(|fallback_error| {
                    format!("{}; fallback: {}", primary_error, fallback_error)
                })?
        }
    };
    let mut result = parse_google_response(&body)?;
    if result.detected_lang.is_none() {
        result.detected_lang = Some(src.to_string());
    }
    result.strategy = working_strategy;
    Ok(result)
}

fn split_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    fn split_segment(mut text: &str, max_chars: usize) -> Vec<String> {
        let separators = ["\n\n", "\n", ". ", "! ", "? ", "; ", ", ", " "];
        let mut chunks = Vec::new();
        while text.chars().count() > max_chars {
            let prefix: String = text.chars().take(max_chars).collect();
            let mut best = 0usize;
            for separator in separators {
                if let Some(position) = prefix.rfind(separator) {
                    best = best.max(position + separator.len());
                }
            }
            if best < max_chars / 3 {
                best = prefix.len();
            }
            let (head, tail) = text.split_at(best);
            chunks.push(head.to_string());
            text = tail;
        }
        if !text.is_empty() {
            chunks.push(text.to_string());
        }
        chunks
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        for part in split_segment(line, max_chars) {
            if !current.is_empty() && current.chars().count() + part.chars().count() > max_chars {
                chunks.push(std::mem::take(&mut current));
            }
            current.push_str(&part);
            if current.chars().count() >= max_chars {
                chunks.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn urlencoding(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "%20".into(),
            _ => format!("%{:02X}", byte),
        })
        .collect()
}

fn parse_google_response(body: &str) -> Result<TranslateResult, String> {
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("JSON parse error: {}", e))?;

    let mut translated = String::new();
    if let Some(arr) = json.as_array() {
        if let Some(sentences) = arr.first().and_then(|v| v.as_array()) {
            for sentence in sentences {
                if let Some(parts) = sentence.as_array() {
                    if let Some(text) = parts.first().and_then(|v| v.as_str()) {
                        translated.push_str(text);
                    }
                }
            }
        }
    }

    let detected_lang = json
        .as_array()
        .and_then(|arr| arr.get(2))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(TranslateResult {
        translated,
        detected_lang,
        source: "Google Translate".into(),
        strategy: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_google_response() {
        let body = r#"[[["Hello","Hola",null,null,1]],null,"es",null,null,null,1,null,null,[["es"],null,[1],["es"]]]"#;
        let result = parse_google_response(body).unwrap();
        assert_eq!(result.translated, "Hello");
        assert_eq!(result.detected_lang, Some("es".into()));
    }

    #[test]
    fn test_split_text_preserves_content() {
        let text = "one. two. three. four.";
        assert_eq!(split_text(text, 7).concat(), text);
    }

    #[test]
    fn builds_and_parses_google_batches_in_order() {
        let texts = vec!["保存".to_string(), "削除".to_string()];
        let payload = build_batch_payload(&texts, 0, 2);
        let translated = payload.replace("保存", "Lưu").replace("削除", "Xóa");
        assert_eq!(
            parse_batch_result(&translated, 0, 2).unwrap(),
            vec!["Lưu", "Xóa"]
        );
    }

    #[test]
    fn rejects_incomplete_google_batch_markers() {
        let error = parse_batch_result("__SBT_ITEM_0000_BEGIN__\nLưu", 0, 1).unwrap_err();
        assert!(error.contains("END"));
    }

    #[tokio::test]
    #[ignore = "requires live Google Translate access"]
    async fn live_translation_smoke() {
        let strategies: &[u8] = if cfg!(target_os = "windows") {
            &[0, 1]
        } else {
            &[0]
        };
        for &strategy in strategies {
            let result = translate("hello", "en", "vi", strategy).await.unwrap();
            assert!(!result.translated.trim().is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "requires live Google Translate access"]
    async fn live_google_batch_smoke() {
        let texts = vec!["保存してください".to_string(), "削除します".to_string()];
        let results = translate_batch(&texts, "ja", "vi", 0).await;
        assert_eq!(results.len(), texts.len());
        assert!(results
            .into_iter()
            .all(|result| result.is_ok_and(|value| !value.translated.trim().is_empty())));
    }
}
