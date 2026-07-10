use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseKind {
    Postgres,
    MySQL,
    #[allow(clippy::upper_case_acronyms)]
    MSSQL,
    SQLite,
    MongoDB,
    #[serde(rename = "ai")]
    AI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub name: String,
    pub kind: DatabaseKind,
    pub uri: String,
    #[serde(default)]
    pub schema: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    #[serde(alias = "openai")]
    #[default]
    OpenAI,
    #[serde(alias = "anthropic")]
    Anthropic,
    #[serde(alias = "gemini")]
    Gemini,
}

impl fmt::Display for AiProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiProvider::OpenAI => write!(f, "openai"),
            AiProvider::Anthropic => write!(f, "anthropic"),
            AiProvider::Gemini => write!(f, "gemini"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub name: String,
    pub uri: String,
    pub api_key: String,
    pub model: String,
    #[serde(default)]
    pub provider: AiProvider,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub temperature: f64,
}

fn default_concurrency() -> usize { 10 }
fn default_timeout_secs() -> u64 { 60 }
fn default_max_tokens() -> u32 { 1024 }

impl fmt::Display for AiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AiConfig {{ name: {}, provider: {}, uri: {}, model: {}, api_key: [redacted] }}",
            self.name, self.provider, self.uri, self.model)
    }
}

pub fn load_config(path: &str) -> anyhow::Result<Vec<ConnectionConfig>> {
    if !std::path::Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let entries: Vec<serde_yaml::Value> = serde_yaml::from_str(&content)?;

    let mut connections = Vec::new();
    for entry in entries {
        let is_ai = entry
            .get("kind")
            .and_then(|v| v.as_str())
            .is_some_and(|k| k.to_lowercase() == "ai");
        if !is_ai {
            let cfg: ConnectionConfig = serde_yaml::from_value(entry)?;
            connections.push(cfg);
        }
    }
    Ok(connections)
}

pub fn load_ai_configs(path: &str) -> anyhow::Result<Vec<AiConfig>> {
    if !std::path::Path::new(path).exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let entries: Vec<serde_yaml::Value> = serde_yaml::from_str(&content)?;

    let mut ai_configs = Vec::new();
    for entry in entries {
        if entry
            .get("kind")
            .and_then(|v| v.as_str())
            .is_some_and(|k| k.to_lowercase() == "ai")
        {
            let cfg: AiConfig = serde_yaml::from_value(entry)?;
            ai_configs.push(cfg);
        }
    }
    Ok(ai_configs)
}
