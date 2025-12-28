//! LLM client for relation extraction
//!
//! This module provides LLM integration using Ollama for advanced
//! relation extraction tasks like "Said" relations from Korean news.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for LLM client
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Ollama endpoint URL (default: http://localhost:11434)
    pub endpoint: String,

    /// Model name to use (default: gemma2:9b)
    pub model: String,

    /// Request timeout in seconds
    pub timeout_secs: u64,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Temperature for generation (0.0 - 1.0)
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434".to_string(),
            model: "qwen2.5:7b".to_string(),
            timeout_secs: 60,
            max_tokens: 2048,
            temperature: 0.1,
        }
    }
}

impl LlmConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        Self {
            endpoint: std::env::var("OLLAMA_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "gemma2:9b".to_string()),
            timeout_secs: std::env::var("OLLAMA_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            max_tokens: std::env::var("OLLAMA_MAX_TOKENS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2048),
            temperature: std::env::var("OLLAMA_TEMPERATURE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.1),
        }
    }
}

/// Ollama generate request
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

/// Ollama generation options
#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: u32,
}

/// Ollama generate response
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
    #[serde(default)]
    done: bool,
}

/// Extracted Said relation from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaidRelation {
    /// Speaker name (person who said it)
    pub speaker: String,

    /// What was said (quoted or paraphrased)
    pub content: String,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,

    /// Evidence sentence from source
    pub evidence: String,
}

/// LLM response for Said extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaidExtractionResponse {
    /// List of extracted Said relations
    #[serde(default)]
    pub relations: Vec<SaidRelation>,
}

/// LLM client for relation extraction
pub struct LlmClient {
    client: Client,
    config: LlmConfig,
}

impl LlmClient {
    /// Create a new LLM client with default config
    pub fn new() -> Result<Self> {
        Self::with_config(LlmConfig::default())
    }

    /// Create a new LLM client with custom config
    pub fn with_config(config: LlmConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, config })
    }

    /// Create a client from environment variables
    pub fn from_env() -> Result<Self> {
        Self::with_config(LlmConfig::from_env())
    }

    /// Check if Ollama is available
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.config.endpoint);
        self.client.get(&url).send().await.is_ok()
    }

    /// Extract Said relations from article text
    pub async fn extract_said_relations(&self, text: &str) -> Result<Vec<SaidRelation>> {
        let prompt = self.build_said_prompt(text);
        let response = self.generate(&prompt).await?;
        self.parse_said_response(&response)
    }

    /// Build prompt for Said relation extraction
    fn build_said_prompt(&self, text: &str) -> String {
        format!(
            r#"당신은 한국어 뉴스 기사에서 발언(Said) 관계를 추출하는 전문가입니다.

다음 뉴스 기사에서 "누가 무엇을 말했는지"를 추출하세요.

## 규칙:
1. 발언자는 실제 인물 이름이어야 합니다 (직책만 있으면 안됨)
2. 발언 내용은 직접 인용 또는 간접 인용 모두 가능합니다
3. 증거는 원문에서 해당 발언을 포함하는 문장입니다
4. 신뢰도는 0.0~1.0 사이 값입니다 (직접인용=0.95, 간접인용=0.8, 추정=0.6)

## 출력 형식 (JSON):
```json
{{
  "relations": [
    {{
      "speaker": "발언자 이름",
      "content": "발언 내용",
      "confidence": 0.9,
      "evidence": "원문에서 발언을 포함하는 문장"
    }}
  ]
}}
```

## 뉴스 기사:
{text}

## 추출된 발언 관계 (JSON):"#
        )
    }

    /// Generate text using Ollama
    async fn generate(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.config.endpoint);

        let request = OllamaRequest {
            model: self.config.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options: OllamaOptions {
                temperature: self.config.temperature,
                num_predict: self.config.max_tokens,
            },
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Ollama")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama request failed: {} - {}", status, body);
        }

        let ollama_response: OllamaResponse = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        Ok(ollama_response.response)
    }

    /// Parse Said extraction response from LLM
    fn parse_said_response(&self, response: &str) -> Result<Vec<SaidRelation>> {
        // Try to extract JSON from response
        let json_str = self.extract_json(response);

        // Debug: log the extracted JSON
        tracing::debug!("Extracted JSON: {}", &json_str[..json_str.len().min(500)]);

        match serde_json::from_str::<SaidExtractionResponse>(&json_str) {
            Ok(parsed) => {
                tracing::debug!("Parsed {} relations", parsed.relations.len());
                Ok(parsed.relations)
            }
            Err(e) => {
                // Try parsing as array directly
                if let Ok(relations) = serde_json::from_str::<Vec<SaidRelation>>(&json_str) {
                    return Ok(relations);
                }

                tracing::warn!(
                    "Failed to parse Said response: {}. Response truncated: {}",
                    e,
                    &response[..response.len().min(200)]
                );
                Ok(Vec::new())
            }
        }
    }

    /// Extract JSON from markdown code blocks or raw text
    fn extract_json(&self, text: &str) -> String {
        let raw_json = self.extract_raw_json(text);

        // Try to fix common JSON issues
        self.fix_json(&raw_json)
    }

    /// Extract raw JSON string from text
    fn extract_raw_json(&self, text: &str) -> String {
        // Try to find JSON in code block
        if let Some(start) = text.find("```json") {
            if let Some(end) = text[start + 7..].find("```") {
                return text[start + 7..start + 7 + end].trim().to_string();
            }
        }

        // Try to find JSON in generic code block
        if let Some(start) = text.find("```") {
            let after_start = &text[start + 3..];
            // Skip language identifier if present
            let content_start = after_start.find('\n').unwrap_or(0) + 1;
            if let Some(end) = after_start[content_start..].find("```") {
                return after_start[content_start..content_start + end]
                    .trim()
                    .to_string();
            }
        }

        // Try to find raw JSON object
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                if end > start {
                    return text[start..=end].to_string();
                }
            }
        }

        text.trim().to_string()
    }

    /// Fix common JSON issues from LLM output
    fn fix_json(&self, json: &str) -> String {
        // First try parsing as-is
        if serde_json::from_str::<serde_json::Value>(json).is_ok() {
            return json.to_string();
        }

        // Fix unescaped quotes inside string values by re-parsing manually
        // This is a simplified approach - extract relations manually

        let mut fixed = String::new();
        let mut in_string = false;
        let mut escape_next = false;
        let mut chars = json.chars().peekable();

        while let Some(c) = chars.next() {
            if escape_next {
                fixed.push(c);
                escape_next = false;
                continue;
            }

            match c {
                '\\' => {
                    escape_next = true;
                    fixed.push(c);
                }
                '"' => {
                    in_string = !in_string;
                    fixed.push(c);
                }
                '\'' if in_string => {
                    // Replace unescaped single quotes with escaped double quotes
                    // or keep as single quotes (which are valid in string content)
                    fixed.push(c);
                }
                _ => {
                    fixed.push(c);
                }
            }
        }

        // If still invalid, try extracting speaker/content pairs manually
        if serde_json::from_str::<serde_json::Value>(&fixed).is_err() {
            return self.extract_relations_manually(json);
        }

        fixed
    }

    /// Manually extract relations when JSON parsing fails
    fn extract_relations_manually(&self, text: &str) -> String {
        let mut relations = Vec::new();

        // Find all speaker patterns
        let speaker_re = regex::Regex::new(r#""speaker"\s*:\s*"([^"]+)""#).unwrap();
        let content_re = regex::Regex::new(r#""content"\s*:\s*"([^"]*(?:[^"\\]|\\.)*)""#).unwrap();
        let confidence_re = regex::Regex::new(r#""confidence"\s*:\s*([\d.]+)"#).unwrap();
        let evidence_re = regex::Regex::new(r#""evidence"\s*:\s*"([^"]*(?:[^"\\]|\\.)*)""#).unwrap();

        // Split by relation blocks (objects starting with {)
        for block in text.split(r#"{"#).skip(1) {
            let block = format!("{{{}", block);

            let speaker = speaker_re
                .captures(&block)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());

            let content = content_re
                .captures(&block)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());

            let confidence = confidence_re
                .captures(&block)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<f32>().ok())
                .unwrap_or(0.8);

            let evidence = evidence_re
                .captures(&block)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            if let (Some(speaker), Some(content)) = (speaker, content) {
                if !speaker.is_empty() && !content.is_empty() {
                    relations.push(SaidRelation {
                        speaker: speaker.replace("\\\"", "\"").replace("\\'", "'"),
                        content: content.replace("\\\"", "\"").replace("\\'", "'"),
                        confidence,
                        evidence: evidence.replace("\\\"", "\"").replace("\\'", "'"),
                    });
                }
            }
        }

        // Convert back to JSON
        match serde_json::to_string(&SaidExtractionResponse { relations }) {
            Ok(json) => json,
            Err(_) => r#"{"relations":[]}"#.to_string(),
        }
    }
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default LlmClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = LlmConfig::default();
        assert_eq!(config.endpoint, "http://localhost:11434");
        assert_eq!(config.model, "gemma2:9b");
    }

    #[test]
    fn test_extract_json_from_code_block() {
        let client = LlmClient::new().unwrap();

        let text = r#"Here is the result:
```json
{"relations": [{"speaker": "홍길동", "content": "테스트", "confidence": 0.9, "evidence": "홍길동 의원은 테스트라고 말했다."}]}
```
"#;
        let json = client.extract_json(text);
        assert!(json.contains("홍길동"));
    }

    #[test]
    fn test_extract_json_raw() {
        let client = LlmClient::new().unwrap();

        let text = r#"{"relations": []}"#;
        let json = client.extract_json(text);
        assert_eq!(json, r#"{"relations": []}"#);
    }

    #[test]
    fn test_parse_said_response() {
        let client = LlmClient::new().unwrap();

        let json = r#"{"relations": [{"speaker": "김철수", "content": "경제가 회복되고 있다", "confidence": 0.9, "evidence": "김철수 장관은 경제가 회복되고 있다고 밝혔다."}]}"#;

        let relations = client.parse_said_response(json).unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].speaker, "김철수");
    }

    #[test]
    fn test_parse_empty_response() {
        let client = LlmClient::new().unwrap();

        let relations = client.parse_said_response("{}").unwrap();
        assert!(relations.is_empty());
    }
}
