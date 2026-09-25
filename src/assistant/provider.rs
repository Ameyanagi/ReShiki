//! Assistant provider selection and credential lookup.
//!
//! Codex remains the default and uses its own CLI sign-in. OpenAI-compatible
//! HTTP endpoints (OpenAI, Ollama, LM Studio, OpenRouter, vLLM, …) and the
//! Anthropic Messages API use an API key from the environment or the OS
//! keychain plus a configurable base URL and model name.
use serde::{Deserialize, Serialize};

/// Supported assistant backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Codex,
    #[serde(alias = "openai-compatible")]
    #[serde(alias = "openai_compatible")]
    #[serde(alias = "custom")]
    OpenAI,
    Anthropic,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::OpenAI => "OpenAI-compatible",
            Self::Anthropic => "Anthropic",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
        }
    }

    /// Default HTTP base URL. Codex uses its local app-server instead.
    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Codex => "",
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com",
        }
    }

    /// Sensible starting model when the user has not chosen one yet.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Codex => super::settings::DEFAULT_MODEL,
            // Widely available structured-output capable default; users with
            // local servers typically override this (e.g. a local Ollama tag).
            Self::OpenAI => "gpt-4o-mini",
            Self::Anthropic => "claude-sonnet-4-5",
        }
    }

    /// Environment variables checked (in order) before the OS keychain.
    pub fn env_names(self) -> &'static [&'static str] {
        match self {
            Self::Codex => &[],
            Self::OpenAI => &["RESHIKI_OPENAI_API_KEY", "OPENAI_API_KEY"],
            Self::Anthropic => &["RESHIKI_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"],
        }
    }

    /// `RESHIKI_*` override for the HTTP base URL.
    pub fn base_url_env(self) -> Option<&'static str> {
        match self {
            Self::Codex => None,
            Self::OpenAI => Some("RESHIKI_OPENAI_BASE_URL"),
            Self::Anthropic => Some("RESHIKI_ANTHROPIC_BASE_URL"),
        }
    }

    fn keychain_account(self) -> Option<&'static str> {
        match self {
            Self::Codex => None,
            Self::OpenAI => Some("openai"),
            Self::Anthropic => Some("anthropic"),
        }
    }

    pub fn needs_api_key(self) -> bool {
        self.keychain_account().is_some()
    }
}

/// Resolve the API key for an HTTP provider.
///
/// Order: `RESHIKI_*` env, generic vendor env (`OPENAI_API_KEY`,
/// `ANTHROPIC_API_KEY`), then the OS keychain entry `reshiki/<account>`.
/// Codex never uses this path; it reuses the CLI sign-in.
pub fn api_key(provider: Provider) -> Result<String, String> {
    for name in provider.env_names() {
        if let Some(value) = std::env::var_os(name)
            .and_then(|v| v.into_string().ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            return Ok(value);
        }
    }
    let Some(account) = provider.keychain_account() else {
        return Err("Codex uses its CLI sign-in, not an API key".into());
    };
    let stored = keyring_entry(account)
        .and_then(|entry| entry.get_password().map_err(|e| e.to_string()))
        .map(|v| v.trim().to_string());
    match stored {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => Err(format!(
            "Missing API key for {}. Set {} or save it in the assistant providers menu.",
            provider.label(),
            provider
                .env_names()
                .first()
                .unwrap_or(&"an environment variable")
        )),
    }
}

/// Returns true when a key is available from env or keychain (no key material).
pub fn has_api_key(provider: Provider) -> bool {
    if !provider.needs_api_key() {
        return true;
    }
    api_key(provider).is_ok()
}

/// Save an API key in the OS keychain under `reshiki/<provider>`.
pub fn set_api_key(provider: Provider, key: &str) -> Result<(), String> {
    let Some(account) = provider.keychain_account() else {
        return Err("Codex uses its CLI sign-in, not an API key".into());
    };
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("Paste an API key first".into());
    }
    if trimmed.len() > 4096 {
        return Err("That API key looks too long".into());
    }
    keyring_entry(account)
        .and_then(|entry| entry.set_password(trimmed).map_err(|e| e.to_string()))
        .map_err(|e| format!("Could not save the API key in the keychain: {e}"))
}

/// Remove a previously saved keychain entry (missing entries are fine).
pub fn delete_api_key(provider: Provider) -> Result<(), String> {
    let Some(account) = provider.keychain_account() else {
        return Err("Codex uses its CLI sign-in, not an API key".into());
    };
    let entry = keyring_entry(account).map_err(|e| format!("Keychain unavailable: {e}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Could not remove the API key: {e}")),
    }
}

fn keyring_entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("reshiki", account).map_err(|e| format!("Keychain unavailable: {e}"))
}

/// Effective base URL: explicit preference, then `RESHIKI_*` env, then default.
pub fn base_url(provider: Provider, configured: &str) -> String {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        return trimmed.trim_end_matches('/').to_string();
    }
    if let Some(env_name) = provider.base_url_env()
        && let Some(value) = std::env::var_os(env_name)
            .and_then(|v| v.into_string().ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    {
        return value.trim_end_matches('/').to_string();
    }
    provider.default_base_url().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_labels_and_defaults_are_stable() {
        assert_eq!(Provider::Codex.label(), "Codex");
        assert_eq!(Provider::OpenAI.label(), "OpenAI-compatible");
        assert_eq!(Provider::Anthropic.label(), "Anthropic");
        assert_eq!(
            Provider::OpenAI.default_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            Provider::Anthropic.default_base_url(),
            "https://api.anthropic.com"
        );
        assert!(!Provider::Codex.needs_api_key());
        assert!(Provider::OpenAI.needs_api_key());
    }

    #[test]
    fn provider_serde_keeps_codex_default_and_aliases() {
        assert_eq!(
            serde_json::from_str::<Provider>("\"codex\"").unwrap(),
            Provider::Codex
        );
        assert_eq!(
            serde_json::from_str::<Provider>("\"openai\"").unwrap(),
            Provider::OpenAI
        );
        assert_eq!(
            serde_json::from_str::<Provider>("\"openai-compatible\"").unwrap(),
            Provider::OpenAI
        );
        assert_eq!(
            serde_json::from_str::<Provider>("\"anthropic\"").unwrap(),
            Provider::Anthropic
        );
        assert!(
            serde_json::from_str::<Provider>("\"unknown\"")
                .err()
                .is_some()
        );
        let default: Provider = serde_json::from_value(serde_json::Value::Null).unwrap_or_default();
        assert_eq!(default, Provider::Codex);
    }

    #[test]
    fn base_url_prefers_explicit_then_default() {
        assert_eq!(
            base_url(Provider::OpenAI, "http://localhost:11434/v1/"),
            "http://localhost:11434/v1"
        );
        assert_eq!(base_url(Provider::OpenAI, ""), "https://api.openai.com/v1");
    }
}
