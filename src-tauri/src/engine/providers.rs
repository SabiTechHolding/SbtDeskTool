use serde::Serialize;

pub const GOOGLE_TRANSLATE: &str = "Google Translate";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub ready: bool,
    pub requires_configuration: bool,
}

/// Registry contract for the provider settings screen. Google Translate is the
/// only production implementation at present; the remaining entries are kept
/// explicit so configuration and availability never have to be hard-coded in
/// the UI later.
pub fn list() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "google",
            name: GOOGLE_TRANSLATE,
            ready: true,
            requires_configuration: false,
        },
        ProviderInfo {
            id: "gemini",
            name: "Gemini",
            ready: true,
            requires_configuration: true,
        },
        ProviderInfo {
            id: "openai",
            name: "OpenAI",
            ready: true,
            requires_configuration: true,
        },
        ProviderInfo {
            id: "claude",
            name: "Claude",
            ready: true,
            requires_configuration: true,
        },
        ProviderInfo {
            id: "deepl",
            name: "DeepL",
            ready: true,
            requires_configuration: true,
        },
        ProviderInfo {
            id: "local",
            name: "Local AI",
            ready: true,
            requires_configuration: true,
        },
    ]
}

#[cfg(test)]
pub fn is_ready(name: &str) -> bool {
    list()
        .into_iter()
        .any(|provider| provider.name == name && provider.ready)
}

#[cfg(test)]
mod tests {
    use super::{is_ready, GOOGLE_TRANSLATE};

    #[test]
    fn google_and_local_provider_implementations_are_available() {
        assert!(is_ready(GOOGLE_TRANSLATE));
        assert!(is_ready("Local AI"));
    }
}
