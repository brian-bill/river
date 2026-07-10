use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Semaphore;

use tracing::{debug, warn};

use crate::adapters::Value;
use crate::connection::{AiConfig, AiProvider};
use crate::engine::planner::AiColumn;
use crate::lang::ast::Expression;

// ── OpenAI-compatible schemas ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiRespMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiRespMessage {
    content: String,
}

// ── Anthropic schemas ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    text: String,
}

// ── Gemini schemas ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GeminiRequest<'a> {
    contents: Vec<GeminiContent<'a>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiContent<'a> {
    role: &'a str,
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Debug, Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    temperature: f64,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiRespContent,
}

#[derive(Debug, Deserialize)]
struct GeminiRespContent {
    parts: Vec<GeminiRespPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiRespPart {
    text: String,
}

// ── AiClient ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AiClient {
    http: reqwest::Client,
}

impl AiClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    pub async fn chat(
        &self,
        config: &AiConfig,
        model: Option<&str>,
        prompt: &str,
    ) -> Result<String, String> {
        match config.provider {
            AiProvider::OpenAI => self.chat_openai(config, model, prompt).await,
            AiProvider::Anthropic => self.chat_anthropic(config, model, prompt).await,
            AiProvider::Gemini => self.chat_gemini(config, model, prompt).await,
        }
    }

    // ── OpenAI-compatible ────────────────────────────────────────────────────

    async fn chat_openai(
        &self,
        config: &AiConfig,
        model: Option<&str>,
        prompt: &str,
    ) -> Result<String, String> {
        let model_name = model.unwrap_or(&config.model);
        let url = format!("{}/chat/completions", config.uri.trim_end_matches('/'));

        let body = OpenAiRequest {
            model: model_name,
            messages: vec![OpenAiMessage {
                role: "user",
                content: prompt,
            }],
            temperature: config.temperature,
            max_tokens: config.max_tokens,
        };

        let mut req = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(config.timeout_secs));

        for (key, value) in &config.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        debug!("[openai] POST {} model={}", url, model_name);

        let body_text = self.send_request(req).await?;
        let parsed: OpenAiResponse = serde_json::from_str(&body_text).map_err(|e| {
            warn!("OpenAI response parse error: {} — body: {}", e, &body_text[..body_text.len().min(500)]);
            "[AI Error: invalid response format]".to_string()
        })?;

        parsed
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| "[AI Error: empty response]".to_string())
    }

    // ── Anthropic ────────────────────────────────────────────────────────────

    async fn chat_anthropic(
        &self,
        config: &AiConfig,
        model: Option<&str>,
        prompt: &str,
    ) -> Result<String, String> {
        let model_name = model.unwrap_or(&config.model);
        let url = format!("{}/v1/messages", config.uri.trim_end_matches('/'));

        let temp = if config.temperature == 0.0 {
            None
        } else {
            Some(config.temperature)
        };

        let body = AnthropicRequest {
            model: model_name,
            messages: vec![AnthropicMessage {
                role: "user",
                content: prompt,
            }],
            max_tokens: config.max_tokens,
            temperature: temp,
        };

        let mut req = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", &config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(config.timeout_secs));

        for (key, value) in &config.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        debug!("[anthropic] POST {} model={}", url, model_name);

        let body_text = self.send_request(req).await?;
        let parsed: AnthropicResponse = serde_json::from_str(&body_text).map_err(|e| {
            warn!("Anthropic response parse error: {} — body: {}", e, &body_text[..body_text.len().min(500)]);
            "[AI Error: invalid response format]".to_string()
        })?;

        parsed
            .content
            .first()
            .map(|b| b.text.clone())
            .ok_or_else(|| "[AI Error: empty response]".to_string())
    }

    // ── Gemini ───────────────────────────────────────────────────────────────

    async fn chat_gemini(
        &self,
        config: &AiConfig,
        model: Option<&str>,
        prompt: &str,
    ) -> Result<String, String> {
        let model_name = model.unwrap_or(&config.model);
        let base = config.uri.trim_end_matches('/');
        let url = format!("{}/models/{}:generateContent", base, model_name);

        let gen_cfg = GeminiGenerationConfig {
            temperature: config.temperature,
            max_output_tokens: config.max_tokens,
        };

        let body = GeminiRequest {
            contents: vec![GeminiContent {
                role: "user",
                parts: vec![GeminiPart { text: prompt }],
            }],
            generation_config: Some(gen_cfg),
        };

        let is_ai_studio = config.uri.contains("generativelanguage.googleapis.com");

        let mut req = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(config.timeout_secs));

        if is_ai_studio {
            req = req.query(&[("key", config.api_key.as_str())]);
        } else {
            req = req.header("Authorization", format!("Bearer {}", config.api_key));
        }

        for (key, value) in &config.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        debug!("[gemini] POST {} model={}", url, model_name);

        let body_text = self.send_request(req).await?;

        if let Ok(err) = serde_json::from_str::<GeminiErrorWrapper>(&body_text)
            && err.error.is_some() {
                let msg = err.error.map(|e| e.message).unwrap_or_default();
                warn!("Gemini API error: {}", msg);
                return Err(format!("[AI Error: {}]", msg));
            }

        let parsed: GeminiResponse = serde_json::from_str(&body_text).map_err(|e| {
            warn!("Gemini response parse error: {} — body: {}", e, &body_text[..body_text.len().min(500)]);
            "[AI Error: invalid response format]".to_string()
        })?;

        parsed
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .ok_or_else(|| "[AI Error: empty response]".to_string())
    }

    // ── shared HTTP helper ───────────────────────────────────────────────────

    async fn send_request(&self, req: reqwest::RequestBuilder) -> Result<String, String> {
        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                "[AI Error: timeout]".to_string()
            } else if e.is_connect() {
                "[AI Error: connection refused]".to_string()
            } else {
                format!("[AI Error: {}]", e)
            }
        })?;

        let status = resp.status();
        if !status.is_success() {
            return if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                Err("[AI Error: authentication failed]".to_string())
            } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                Err("[AI Error: rate limited]".to_string())
            } else {
                Err(format!("[AI Error: HTTP {}]", status.as_u16()))
            };
        }

        resp.text().await.map_err(|e| {
            format!("[AI Error: failed to read response: {}]", e)
        })
    }
}

impl Default for AiClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── Gemini error response ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GeminiErrorDetail {
    message: String,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorWrapper {
    error: Option<GeminiErrorDetail>,
}

// ── Batch execution ──────────────────────────────────────────────────────────

pub type EvalExprFn = fn(&Expression, &[String], &[Option<String>], &[Value]) -> Value;

pub async fn execute_ai_columns(
    client: &AiClient,
    ai_configs: Arc<HashMap<String, AiConfig>>,
    ai_columns: &[AiColumn],
    base_columns: &[String],
    column_sources: &[Option<String>],
    rows: &[Vec<Value>],
    eval_expr_fn: EvalExprFn,
) -> Vec<Vec<Value>> {
    let mut new_rows: Vec<Vec<Value>> = rows.to_vec();

    for col in ai_columns {
        let Expression::AiQuery { config: config_name, model, prompt } = &col.expr else {
            for row in &mut new_rows {
                row.push(Value::Null);
            }
            continue;
        };

        let ai_config = match ai_configs.get(config_name) {
            Some(c) => c.clone(),
            None => {
                for row in &mut new_rows {
                    row.push(Value::String(format!(
                        "[AI Error: unknown config '{}']",
                        config_name
                    )));
                }
                continue;
            }
        };

        let concurrency = ai_config.concurrency.max(1);
        let semaphore = Arc::new(Semaphore::new(concurrency));

        let mut tasks = Vec::with_capacity(rows.len());

        for row in rows {
            let prompt_val = eval_expr_fn(prompt, base_columns, column_sources, row);
            let prompt_str = match &prompt_val {
                Value::Null => {
                    tasks.push(None);
                    continue;
                }
                Value::String(s) => s.clone(),
                Value::Int(n) => n.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Bool(b) => b.to_string(),
            };

            let cfg = ai_config.clone();
            let mdl = model.clone();
            let sem = semaphore.clone();
            let http = client.http.clone();

            tasks.push(Some(tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");
                let temp = AiClient { http };
                match temp.chat(&cfg, mdl.as_deref(), &prompt_str).await {
                    Ok(content) => Value::String(content),
                    Err(err_msg) => Value::String(err_msg),
                }
            })));
        }

        let mut results = Vec::with_capacity(tasks.len());
        for task in tasks {
            let result = match task {
                Some(handle) => match handle.await {
                    Ok(val) => val,
                    Err(e) => Value::String(format!("[AI Error: task failed: {}]", e)),
                },
                None => Value::Null,
            };
            results.push(result);
        }

        for (i, result) in results.into_iter().enumerate() {
            new_rows[i].push(result);
        }
    }

    new_rows
}
