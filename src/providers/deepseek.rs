// Copyright 2025 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// DeepSeek provider implementation

use super::{AiProvider, ProviderExchange, ProviderResponse, TokenUsage};
use crate::config::Config;
use crate::log_debug;
use crate::session::Message;
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;

/// DeepSeek pricing constants (per 1M tokens in USD)
/// Update according to https://platform.deepseek.com/pricing if needed
const PRICING: &[(&str, f64, f64)] = &[
    // Model, Input price per 1M tokens, Output price per 1M tokens
    ("deepseek-chat", 0.20, 0.40), // DeepSeek-V2 Chat
    ("deepseek-coder", 0.20, 0.40), // DeepSeek-V2 Coder
    // Add more DeepSeek models as released
];

/// Calculate cost for DeepSeek models
fn calculate_cost(model: &str, prompt_tokens: u64, completion_tokens: u64) -> Option<f64> {
    for (pricing_model, input_price, output_price) in PRICING {
        if model.contains(pricing_model) {
            let input_cost = (prompt_tokens as f64 / 1_000_000.0) * input_price;
            let output_cost = (completion_tokens as f64 / 1_000_000.0) * output_price;
            return Some(input_cost + output_cost);
        }
    }
    None
}

/// Check if a model supports the temperature parameter
fn supports_temperature(_model: &str) -> bool {
    true // All DeepSeek models support temperature as of June 2025
}

/// DeepSeek provider implementation
pub struct DeepSeekProvider;

impl Default for DeepSeekProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DeepSeekProvider {
    pub fn new() -> Self {
        Self
    }
}

// Constants
const DEEPSEEK_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";
const DEEPSEEK_API_URL: &str = "https://api.deepseek.com/v1/chat/completions";

/// Message format for the DeepSeek API (compatible with OpenAI format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekMessage {
    pub role: String,
    pub content: serde_json::Value, // Can be string or array with content parts
}

#[async_trait::async_trait]
impl AiProvider for DeepSeekProvider {
    fn name(&self) -> &str {
        "deepseek"
    }

    fn supports_model(&self, model: &str) -> bool {
        // DeepSeek models
        model.starts_with("deepseek-chat") || model.starts_with("deepseek-coder")
    }

    fn get_api_key(&self, _config: &Config) -> Result<String> {
        // API keys from environment variable
        match env::var(DEEPSEEK_API_KEY_ENV) {
            Ok(key) => Ok(key),
            Err(_) => Err(anyhow::anyhow!(
                "DeepSeek API key not found in environment variable: {}",
                DEEPSEEK_API_KEY_ENV
            )),
        }
    }

    fn supports_caching(&self, _model: &str) -> bool {
        false
    }

    fn supports_vision(&self, _model: &str) -> bool {
        false // DeepSeek does not support vision as of now
    }

    fn get_max_input_tokens(&self, model: &str) -> usize {
        // DeepSeek-V2 models: 128K context window
        if model.contains("deepseek") {
            return 128_000;
        }
        8_192 // fallback
    }

    async fn chat_completion(
        &self,
        messages: &[Message],
        model: &str,
        temperature: f32,
        config: &Config,
        cancellation_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<ProviderResponse> {
        // Check for cancellation before starting
        if let Some(ref token) = cancellation_token {
            if token.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(anyhow::anyhow!("Request cancelled before starting"));
            }
        }
        // Get API key
        let api_key = self.get_api_key(config)?;

        // Convert messages to DeepSeek format (OpenAI compatible)
        let deepseek_messages = convert_messages(messages);

        // Create the request body
        let mut request_body = serde_json::json!({
            "model": model,
            "messages": deepseek_messages,
        });

        // Add temperature when supported
        if supports_temperature(model) {
            request_body["temperature"] = serde_json::json!(temperature);
        }

        // Create HTTP client
        let client = Client::new();

        // Track API request time
        let api_start = std::time::Instant::now();

        // Make the actual API request
        let response = client
            .post(DEEPSEEK_API_URL)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        // Calculate API request time
        let api_duration = api_start.elapsed();
        let api_time_ms = api_duration.as_millis() as u64;

        // Get response status
        let status = response.status();

        // Get response body as text first for debugging
        let response_text = response.text().await?;

        // Parse the text to JSON
        let response_json: serde_json::Value = match serde_json::from_str(&response_text) {
            Ok(json) => json,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to parse response JSON: {}. Response: {}",
                    e,
                    response_text
                ));
            }
        };

        // Handle error responses
        if !status.is_success() {
            let mut error_details = Vec::new();
            error_details.push(format!("HTTP {}", status));

            if let Some(error_obj) = response_json.get("error") {
                if let Some(msg) = error_obj.get("message").and_then(|m| m.as_str()) {
                    error_details.push(format!("Message: {}", msg));
                }
                if let Some(code) = error_obj.get("code").and_then(|c| c.as_str()) {
                    error_details.push(format!("Code: {}", code));
                }
            }
            if error_details.len() == 1 {
                error_details.push(format!("Raw response: {}", response_text));
            }
            let full_error = error_details.join(" | ");
            return Err(anyhow::anyhow!("DeepSeek API error: {}", full_error));
        }

        // Extract content
        let message = response_json
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .ok_or_else(|| {
                anyhow::anyhow!("Invalid response format from DeepSeek: {}", response_text)
            })?;

        // Extract finish_reason
        let finish_reason = response_json
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(|fr| fr.as_str())
            .map(|s| s.to_string());

        if let Some(ref reason) = finish_reason {
            log_debug!("Finish reason: {}", reason);
        }

        let mut content = String::new();
        if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
            content = text.to_string();
        }

        // DeepSeek does not support function/tool calls (as of June 2025)
        let tool_calls = None;

        // Extract token usage
        let usage: Option<TokenUsage> = if let Some(usage_obj) = response_json.get("usage") {
            let prompt_tokens = usage_obj
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let completion_tokens = usage_obj
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total_tokens = usage_obj
                .get("total_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let cost = calculate_cost(model, prompt_tokens, completion_tokens);

            Some(TokenUsage {
                prompt_tokens,
                output_tokens: completion_tokens,
                total_tokens,
                cached_tokens: 0,
                cost,
                request_time_ms: Some(api_time_ms),
            })
        } else {
            None
        };

        // Create exchange record
        let exchange = ProviderExchange::new(request_body, response_json, usage, self.name());

        Ok(ProviderResponse {
            content,
            exchange,
            tool_calls,
            finish_reason,
        })
    }
}

// Convert our session messages to DeepSeek format (OpenAI compatible)
fn convert_messages(messages: &[Message]) -> Vec<DeepSeekMessage> {
    let mut result = Vec::new();
    for msg in messages {
        result.push(DeepSeekMessage {
            role: msg.role.clone(),
            content: serde_json::json!(msg.content),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_temperature() {
        assert!(supports_temperature("deepseek-chat"));
        assert!(supports_temperature("deepseek-coder"));
    }

    #[test]
    fn test_supports_model() {
        let provider = DeepSeekProvider::new();
        assert!(provider.supports_model("deepseek-chat"));
        assert!(provider.supports_model("deepseek-coder"));
        assert!(!provider.supports_model("gpt-4"));
    }

    #[test]
    fn test_calculate_cost() {
        // 1000 input, 1000 output tokens for deepseek-chat
        let cost = calculate_cost("deepseek-chat", 1000, 1000).unwrap();
        // Should be 0.0002 + 0.0004 = 0.0006 USD
        assert!((cost - 0.0006).abs() < 1e-6);
    }
}
