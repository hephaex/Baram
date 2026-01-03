//! LLM client for relation extraction
//!
//! This module provides LLM integration using vLLM (OpenAI-compatible API) or Ollama
//! for advanced relation extraction tasks like "Said" relations from Korean news.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// LLM backend type
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LlmBackend {
    /// vLLM with OpenAI-compatible API (default)
    #[default]
    Vllm,
    /// Ollama API
    Ollama,
}

impl std::str::FromStr for LlmBackend {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "ollama" => LlmBackend::Ollama,
            _ => LlmBackend::Vllm,
        })
    }
}

/// Configuration for LLM client
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// LLM backend to use
    pub backend: LlmBackend,

    /// API endpoint URL
    pub endpoint: String,

    /// Model name to use
    pub model: String,

    /// Request timeout in seconds
    pub timeout_secs: u64,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Temperature for generation (0.0 - 1.0)
    pub temperature: f32,

    /// Maximum retry attempts for failed requests
    pub max_retries: u32,

    /// Initial retry delay in milliseconds (doubles with each retry)
    pub retry_delay_ms: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            backend: LlmBackend::Vllm,
            endpoint: "http://localhost:8002".to_string(),
            model: "qwen2.5".to_string(),
            timeout_secs: 120,
            max_tokens: 1024,
            temperature: 0.1,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

impl LlmConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        let backend = std::env::var("LLM_BACKEND")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(LlmBackend::Vllm);

        let (default_endpoint, default_model) = match backend {
            LlmBackend::Vllm => ("http://localhost:8002", "qwen2.5"),
            LlmBackend::Ollama => ("http://localhost:11434", "qwen2.5:7b"),
        };

        Self {
            backend,
            endpoint: std::env::var("LLM_ENDPOINT").unwrap_or_else(|_| default_endpoint.to_string()),
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| default_model.to_string()),
            timeout_secs: std::env::var("LLM_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            max_tokens: std::env::var("LLM_MAX_TOKENS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1024),
            temperature: std::env::var("LLM_TEMPERATURE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.1),
            max_retries: std::env::var("LLM_MAX_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            retry_delay_ms: std::env::var("LLM_RETRY_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
        }
    }
}

// ============================================================================
// OpenAI-compatible API structures (for vLLM)
// ============================================================================

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: u32,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessageResponse,
}

#[derive(Debug, Deserialize)]
struct OpenAIMessageResponse {
    content: String,
}

// ============================================================================
// Ollama API structures
// ============================================================================

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: u32,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

// ============================================================================
// Said relation extraction types
// ============================================================================

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

/// Article info for batch processing
#[derive(Debug, Clone)]
pub struct ArticleInfo {
    /// Article ID
    pub id: String,
    /// Article title
    pub title: String,
    /// Article content
    pub content: String,
}

/// Batch extraction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSaidResult {
    /// Article ID
    #[serde(default)]
    pub article_id: String,
    /// Extracted relations for this article
    #[serde(default)]
    pub relations: Vec<SaidRelation>,
}

/// LLM response for Said extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaidExtractionResponse {
    /// List of extracted Said relations
    #[serde(default)]
    pub relations: Vec<SaidRelation>,
}

// ============================================================================
// LLM Client
// ============================================================================

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

    /// Get the backend type
    pub fn backend(&self) -> &LlmBackend {
        &self.config.backend
    }

    /// Check if LLM service is available
    pub async fn is_available(&self) -> bool {
        let url = match self.config.backend {
            LlmBackend::Vllm => format!("{}/health", self.config.endpoint),
            LlmBackend::Ollama => format!("{}/api/tags", self.config.endpoint),
        };
        self.client.get(&url).send().await.is_ok()
    }

    /// Extract Said relations from article text
    pub async fn extract_said_relations(&self, text: &str) -> Result<Vec<SaidRelation>> {
        let prompt = self.build_said_prompt(text);
        let response = self.generate(&prompt).await?;
        self.parse_said_response(&response)
    }

    /// Extract Said relations from multiple articles in batch
    pub async fn extract_said_batch(
        &self,
        articles: &[ArticleInfo],
    ) -> Result<std::collections::HashMap<String, Vec<SaidRelation>>> {
        if articles.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let prompt = self.build_batch_prompt(articles);
        let response = self.generate(&prompt).await?;
        self.parse_batch_response(&response, articles)
    }

    /// Generate text using the configured backend with retry logic
    async fn generate(&self, prompt: &str) -> Result<String> {
        let mut last_error: Option<anyhow::Error> = None;
        let mut delay_ms = self.config.retry_delay_ms;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                tracing::warn!(
                    attempt = attempt,
                    max_retries = self.config.max_retries,
                    delay_ms = delay_ms,
                    "Retrying LLM request after failure"
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                delay_ms = (delay_ms * 2).min(30000);
            }

            let result = match self.config.backend {
                LlmBackend::Vllm => self.generate_vllm(prompt).await,
                LlmBackend::Ollama => self.generate_ollama(prompt).await,
            };

            match result {
                Ok(response) => {
                    if attempt > 0 {
                        tracing::info!(attempt = attempt, "LLM request succeeded after retry");
                    }
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("LLM request failed after all retries")))
    }

    /// Generate using vLLM (OpenAI-compatible API)
    async fn generate_vllm(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/v1/chat/completions", self.config.endpoint);

        let request = OpenAIRequest {
            model: self.config.model.clone(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to vLLM")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("vLLM request failed: {status} - {body}");
        }

        let openai_response: OpenAIResponse = response
            .json()
            .await
            .context("Failed to parse vLLM response")?;

        openai_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow::anyhow!("No response from vLLM"))
    }

    /// Generate using Ollama API
    async fn generate_ollama(&self, prompt: &str) -> Result<String> {
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
            anyhow::bail!("Ollama request failed: {status} - {body}");
        }

        let ollama_response: OllamaResponse = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        Ok(ollama_response.response)
    }

    /// Build prompt for batch Said relation extraction
    fn build_batch_prompt(&self, articles: &[ArticleInfo]) -> String {
        let mut articles_text = String::new();
        for (i, article) in articles.iter().enumerate() {
            let content = if article.content.chars().count() > 1000 {
                let truncated: String = article.content.chars().take(1000).collect();
                format!("{truncated}...")
            } else {
                article.content.clone()
            };
            articles_text.push_str(&format!(
                "\n### [기사 {}] ID: {}\n제목: {}\n내용: {}\n",
                i + 1,
                article.id,
                article.title,
                content
            ));
        }

        format!(
            r#"당신은 한국어 뉴스 기사에서 발언(Said) 관계를 추출하는 전문가입니다.

다음 여러 뉴스 기사에서 "누가 무엇을 말했는지"를 각각 추출하세요.

## 규칙:
1. 발언자는 실제 인물 이름이어야 합니다
2. 각 기사별로 article_id를 반드시 포함하세요
3. 발언이 없는 기사는 빈 배열로 표시하세요
4. 신뢰도: 직접인용=0.95, 간접인용=0.8

## 출력 형식 (JSON 배열):
```json
[
  {{
    "article_id": "기사ID",
    "relations": [
      {{"speaker": "이름", "content": "발언", "confidence": 0.9, "evidence": "근거문장"}}
    ]
  }}
]
```

## 뉴스 기사들:
{articles_text}

## 추출 결과 (JSON):"#
        )
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

    /// Parse batch response from LLM
    fn parse_batch_response(
        &self,
        response: &str,
        articles: &[ArticleInfo],
    ) -> Result<std::collections::HashMap<String, Vec<SaidRelation>>> {
        let mut results = std::collections::HashMap::new();

        for article in articles {
            results.insert(article.id.clone(), Vec::new());
        }

        let json_str = self.extract_json(response);

        if let Ok(batch_results) = serde_json::from_str::<Vec<BatchSaidResult>>(&json_str) {
            for result in batch_results {
                if !result.article_id.is_empty() {
                    results.insert(result.article_id, result.relations);
                }
            }
            return Ok(results);
        }

        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(arr) = obj
                .get("results")
                .or(obj.get("articles"))
                .and_then(|v| v.as_array())
            {
                for item in arr {
                    if let (Some(id), Some(rels)) = (
                        item.get("article_id").and_then(|v| v.as_str()),
                        item.get("relations").and_then(|v| v.as_array()),
                    ) {
                        let relations: Vec<SaidRelation> = rels
                            .iter()
                            .filter_map(|r| serde_json::from_value(r.clone()).ok())
                            .collect();
                        results.insert(id.to_string(), relations);
                    }
                }
                return Ok(results);
            }
        }

        self.parse_batch_manually(response, articles, &mut results);
        Ok(results)
    }

    /// Manually parse batch response when JSON parsing fails
    fn parse_batch_manually(
        &self,
        text: &str,
        articles: &[ArticleInfo],
        results: &mut std::collections::HashMap<String, Vec<SaidRelation>>,
    ) {
        let article_id_re = regex::Regex::new(r#""article_id"\s*:\s*"([^"]+)""#).unwrap();
        let blocks: Vec<&str> = text.split(r#""article_id""#).collect();

        for (i, block) in blocks.iter().enumerate().skip(1) {
            let block_with_key = format!(r#""article_id"{block}"#);
            if let Some(cap) = article_id_re.captures(&block_with_key) {
                if let Some(id) = cap.get(1) {
                    let article_id = id.as_str().to_string();
                    let relations_json = self.extract_relations_manually(&block_with_key);
                    if let Ok(parsed) =
                        serde_json::from_str::<SaidExtractionResponse>(&relations_json)
                    {
                        results.insert(article_id, parsed.relations);
                    }
                }
            } else if i <= articles.len() {
                let article_id = articles[i - 1].id.clone();
                let relations_json = self.extract_relations_manually(block);
                if let Ok(parsed) = serde_json::from_str::<SaidExtractionResponse>(&relations_json)
                {
                    if !parsed.relations.is_empty() {
                        results.insert(article_id, parsed.relations);
                    }
                }
            }
        }
    }

    /// Parse Said extraction response from LLM
    fn parse_said_response(&self, response: &str) -> Result<Vec<SaidRelation>> {
        let json_str = self.extract_json(response);

        tracing::debug!("Extracted JSON: {}", &json_str[..json_str.len().min(500)]);

        match serde_json::from_str::<SaidExtractionResponse>(&json_str) {
            Ok(parsed) => {
                tracing::debug!("Parsed {} relations", parsed.relations.len());
                Ok(parsed.relations)
            }
            Err(e) => {
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
        self.fix_json(&raw_json)
    }

    fn extract_raw_json(&self, text: &str) -> String {
        if let Some(start) = text.find("```json") {
            if let Some(end) = text[start + 7..].find("```") {
                return text[start + 7..start + 7 + end].trim().to_string();
            }
        }

        if let Some(start) = text.find("```") {
            let after_start = &text[start + 3..];
            let content_start = after_start.find('\n').unwrap_or(0) + 1;
            if let Some(end) = after_start[content_start..].find("```") {
                return after_start[content_start..content_start + end]
                    .trim()
                    .to_string();
            }
        }

        if let Some(start) = text.find('[') {
            if let Some(end) = text.rfind(']') {
                if end > start {
                    return text[start..=end].to_string();
                }
            }
        }

        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                if end > start {
                    return text[start..=end].to_string();
                }
            }
        }

        text.trim().to_string()
    }

    fn fix_json(&self, json: &str) -> String {
        if serde_json::from_str::<serde_json::Value>(json).is_ok() {
            return json.to_string();
        }

        let mut fixed = String::new();
        let mut in_string = false;
        let mut escape_next = false;

        for c in json.chars() {
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
                _ => {
                    fixed.push(c);
                }
            }
        }

        if serde_json::from_str::<serde_json::Value>(&fixed).is_err() {
            return self.extract_relations_manually(json);
        }

        fixed
    }

    fn extract_relations_manually(&self, text: &str) -> String {
        let mut relations = Vec::new();

        let speaker_re = regex::Regex::new(r#""speaker"\s*:\s*"([^"]+)""#).unwrap();
        let content_re = regex::Regex::new(r#""content"\s*:\s*"([^"]*(?:[^"\\]|\\.)*)""#).unwrap();
        let confidence_re = regex::Regex::new(r#""confidence"\s*:\s*([\d.]+)"#).unwrap();
        let evidence_re = regex::Regex::new(r#""evidence"\s*:\s*"([^"]*(?:[^"\\]|\\.)*)""#).unwrap();

        for block in text.split(r#"{"#).skip(1) {
            let block = format!("{{{block}");

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

        match serde_json::to_string(&SaidExtractionResponse { relations }) {
            Ok(json) => json,
            Err(_) => r#"{"relations":[]}"#.to_string(),
        }
    }
}

// NOTE: LlmClient intentionally does NOT implement Default trait.
// Creating an LlmClient requires network resources and can fail.
// Use LlmClient::new() or LlmClient::with_config() instead.
// This follows Rust API Guidelines C-CTOR:
// https://rust-lang.github.io/api-guidelines/interoperability.html#c-ctor

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = LlmConfig::default();
        assert_eq!(config.endpoint, "http://localhost:8002");
        assert_eq!(config.model, "qwen2.5");
        assert_eq!(config.backend, LlmBackend::Vllm);
    }

    #[test]
    fn test_backend_from_str() {
        assert_eq!("ollama".parse::<LlmBackend>().unwrap(), LlmBackend::Ollama);
        assert_eq!("vllm".parse::<LlmBackend>().unwrap(), LlmBackend::Vllm);
        assert_eq!("openai".parse::<LlmBackend>().unwrap(), LlmBackend::Vllm);
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
    fn test_parse_said_response() {
        let client = LlmClient::new().unwrap();

        let json = r#"{"relations": [{"speaker": "김철수", "content": "경제가 회복되고 있다", "confidence": 0.9, "evidence": "김철수 장관은 경제가 회복되고 있다고 밝혔다."}]}"#;

        let relations = client.parse_said_response(json).unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].speaker, "김철수");
    }
}
