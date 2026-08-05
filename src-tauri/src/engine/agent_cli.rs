use serde::Serialize;
use std::{env, path::Path};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCliProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub executable: String,
    pub arguments: &'static str,
    pub installed: bool,
    pub description: &'static str,
}

struct ProfileDefinition {
    id: &'static str,
    name: &'static str,
    command: &'static str,
    arguments: &'static str,
    description: &'static str,
}

const DEFINITIONS: &[ProfileDefinition] = &[
    ProfileDefinition {
        id: "codex",
        name: "Codex CLI",
        command: "codex",
        arguments: "exec\n--ephemeral\n--color\nnever\n--sandbox\nread-only\n--skip-git-repo-check\n-",
        description: "Runs a one-off Codex session in read-only, ephemeral mode.",
    },
    ProfileDefinition {
        id: "claude",
        name: "Claude Code",
        command: "claude",
        arguments: "-p\nRead the complete translation request from stdin and follow it exactly. Return only the translation.\n--output-format\ntext\n--max-turns\n1",
        description: "Uses Claude Code print mode with one agent turn and prompt data on stdin.",
    },
    ProfileDefinition {
        id: "gemini",
        name: "Gemini CLI",
        command: "gemini",
        arguments: "--output-format\njson",
        description: "Uses Gemini headless mode with JSON output and prompt data on stdin.",
    },
    ProfileDefinition {
        id: "kiro",
        name: "Kiro CLI",
        command: "kiro-cli",
        arguments: "chat\n--no-interactive\n--wrap\nnever\n{prompt}",
        description: "Uses Kiro's non-interactive chat mode and removes terminal formatting.",
    },
    ProfileDefinition {
        id: "opencode",
        name: "OpenCode",
        command: "opencode",
        arguments: "run\n--format\njson\n{prompt}",
        description: "Uses OpenCode run mode and extracts the final text event from JSONL output.",
    },
    ProfileDefinition {
        id: "copilot",
        name: "GitHub Copilot CLI",
        command: "copilot",
        arguments: "-s\n--no-color\n--output-format=text\n--no-custom-instructions\n--no-ask-user",
        description: "Uses Copilot's silent programmatic mode with prompt data on stdin.",
    },
    ProfileDefinition {
        id: "qwen",
        name: "Qwen Code",
        command: "qwen",
        arguments: "--output-format\ntext\n--safe-mode\n--max-session-turns\n1\n--max-wall-time\n60s",
        description: "Uses Qwen headless safe mode with one turn and prompt data on stdin.",
    },
];

pub fn profiles() -> Vec<AgentCliProfile> {
    DEFINITIONS
        .iter()
        .map(|profile| {
            let resolved = find_executable(profile.command);
            AgentCliProfile {
                id: profile.id,
                name: profile.name,
                executable: resolved.as_deref().unwrap_or(profile.command).to_string(),
                arguments: profile.arguments,
                installed: resolved.is_some(),
                description: profile.description,
            }
        })
        .collect()
}

fn find_executable(command: &str) -> Option<String> {
    let direct = Path::new(command);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct.to_string_lossy().into_owned());
    }
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        #[cfg(windows)]
        let extensions = ["", ".exe", ".cmd", ".bat", ".com"];
        #[cfg(not(windows))]
        let extensions = [""];
        for extension in extensions {
            let candidate = directory.join(format!("{command}{extension}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn executable_id(executable: &str) -> String {
    Path::new(executable)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(executable)
        .to_ascii_lowercase()
}

fn profile_name(id: &str) -> &str {
    DEFINITIONS
        .iter()
        .find(|profile| profile.command == id || profile.id == id)
        .map(|profile| profile.name)
        .unwrap_or("Agent CLI")
}

pub fn parse_output(executable: &str, stdout: &str) -> Result<String, String> {
    let cleaned = strip_ansi(stdout).replace('\r', "");
    let id = executable_id(executable);
    let parsed = match id.as_str() {
        "gemini" => parse_gemini_json(&cleaned)?,
        "opencode" => parse_opencode_jsonl(&cleaned)?,
        "kiro-cli" | "kiro" => clean_kiro_output(&cleaned),
        _ => cleaned.trim().to_string(),
    };
    if parsed.trim().is_empty() {
        Err(format!(
            "{} returned no final translation",
            profile_name(&id)
        ))
    } else {
        Ok(parsed.trim().to_string())
    }
}

fn parse_gemini_json(output: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(output.trim())
        .map_err(|error| format!("Gemini CLI returned invalid JSON: {error}"))?;
    if let Some(message) = value
        .pointer("/error/message")
        .and_then(|value| value.as_str())
    {
        return Err(format!("Gemini CLI error: {message}"));
    }
    value
        .get("response")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Gemini CLI returned no final translation".into())
}

fn parse_opencode_jsonl(output: &str) -> Result<String, String> {
    let mut final_text = None;
    let mut reported_error = None;
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|error| format!("OpenCode returned invalid JSONL: {error}"))?;
        match value.get("type").and_then(|value| value.as_str()) {
            Some("text") => {
                if let Some(text) = value.pointer("/part/text").and_then(|value| value.as_str()) {
                    if !text.trim().is_empty() {
                        final_text = Some(text.trim().to_string());
                    }
                }
            }
            Some("error") => {
                reported_error = value
                    .pointer("/error/data/message")
                    .or_else(|| value.pointer("/error/message"))
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
            }
            _ => {}
        }
    }
    final_text.ok_or_else(|| {
        reported_error.unwrap_or_else(|| "OpenCode returned no final translation".into())
    })
}

fn clean_kiro_output(output: &str) -> String {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return String::new();
    };
    let first = first.strip_prefix("> ").unwrap_or(first);
    std::iter::once(first)
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0x1b {
            output.push(bytes[index]);
            index += 1;
            continue;
        }
        index += 1;
        match bytes.get(index).copied() {
            Some(b'[') => {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            Some(b']') => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == 0x07 {
                        index += 1;
                        break;
                    }
                    if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&0x5c) {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            Some(_) => index += 1,
            None => {}
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{parse_output, profiles};

    #[test]
    fn registry_contains_popular_safe_headless_profiles() {
        let profiles = profiles();
        let ids = profiles
            .iter()
            .map(|profile| profile.id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec!["codex", "claude", "gemini", "kiro", "opencode", "copilot", "qwen"]
        );
        for profile in profiles {
            assert!(!profile.arguments.contains("dangerously"));
            assert!(!profile.arguments.contains("--yolo"));
        }
    }

    #[test]
    fn strips_kiro_terminal_formatting_and_prompt_marker() {
        let output = "\u{1b}[38;5;141m> \u{1b}[0mXin chào thế giới.\r\n";
        assert_eq!(
            parse_output("kiro-cli.exe", output).expect("Kiro output"),
            "Xin chào thế giới."
        );
    }

    #[test]
    fn extracts_gemini_json_response() {
        let output = serde_json::json!({"response": "Xin chào", "stats": {}}).to_string();
        assert_eq!(
            parse_output("gemini.cmd", &output).expect("Gemini output"),
            "Xin chào"
        );
    }

    #[test]
    fn extracts_last_opencode_text_event() {
        let output = [
            serde_json::json!({"type": "step_start", "part": {}}).to_string(),
            serde_json::json!({"type": "text", "part": {"text": "Xin chào"}}).to_string(),
        ]
        .join("\n");
        assert_eq!(
            parse_output("opencode.exe", &output).expect("OpenCode output"),
            "Xin chào"
        );
    }

    #[test]
    fn rejects_opencode_run_without_final_text() {
        let output = serde_json::json!({"type": "step_start", "part": {}}).to_string();
        assert_eq!(
            parse_output("opencode.exe", &output).expect_err("missing final text"),
            "OpenCode returned no final translation"
        );
    }
}
