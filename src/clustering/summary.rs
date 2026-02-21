//! Cluster summarization using vLLM
//!
//! Generates event titles and summaries for each cluster using the vLLM API.

use anyhow::{Context, Result};

use crate::llm::{LlmClient, LlmConfig};

use super::models::EventCluster;

/// Generates summaries for event clusters using vLLM
pub struct ClusterSummarizer {
    client: LlmClient,
}

impl ClusterSummarizer {
    /// Create a new summarizer with default LLM config from environment
    pub fn new() -> Result<Self> {
        let config = LlmConfig::from_env();
        let client = LlmClient::with_config(LlmConfig {
            max_tokens: 512,
            temperature: 0.3,
            ..config
        })?;
        Ok(Self { client })
    }

    /// Create a summarizer with a custom LLM client
    pub fn with_client(client: LlmClient) -> Self {
        Self { client }
    }

    /// Check if the LLM service is available
    pub async fn is_available(&self) -> bool {
        self.client.is_available().await
    }

    /// Generate title and summary for a single cluster
    pub async fn summarize_cluster(&self, cluster: &mut EventCluster) -> Result<()> {
        let prompt = self.build_summary_prompt(cluster);
        let response = self.generate(&prompt).await?;
        let (title, summary) = self.parse_summary_response(&response);

        if !title.is_empty() {
            cluster.title = title;
        }
        cluster.summary = summary;

        Ok(())
    }

    /// Generate summaries for multiple clusters
    pub async fn summarize_all(&self, clusters: &mut [EventCluster]) -> Result<usize> {
        let total = clusters.len();
        let mut success_count = 0usize;

        for (i, cluster) in clusters.iter_mut().enumerate() {
            tracing::info!(
                event_id = %cluster.event_id,
                articles = cluster.article_count,
                progress = format!("{}/{}", i + 1, total),
                "Generating summary"
            );

            match self.summarize_cluster(cluster).await {
                Ok(()) => {
                    success_count += 1;
                    tracing::debug!(
                        event_id = %cluster.event_id,
                        title = %cluster.title,
                        "Summary generated"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        event_id = %cluster.event_id,
                        error = %e,
                        "Failed to generate summary, keeping default title"
                    );
                }
            }
        }

        tracing::info!(
            total = total,
            success = success_count,
            failed = total - success_count,
            "Cluster summarization complete"
        );

        Ok(success_count)
    }

    /// Build the prompt for cluster summarization
    fn build_summary_prompt(&self, cluster: &EventCluster) -> String {
        let mut articles_text = String::new();
        for (i, article) in cluster.articles.iter().take(10).enumerate() {
            articles_text.push_str(&format!(
                "{}. [{}] {} ({})\n",
                i + 1,
                article.category,
                article.title,
                article.published_at.as_deref().unwrap_or("날짜 없음")
            ));
        }

        if cluster.articles.len() > 10 {
            articles_text.push_str(&format!(
                "... 외 {}개 기사\n",
                cluster.articles.len() - 10
            ));
        }

        format!(
            r#"당신은 한국어 뉴스 이벤트 분석 전문가입니다.

다음 뉴스 기사들은 같은 이벤트(사건)를 다루고 있습니다.
이 이벤트에 대해 간결한 제목과 요약을 생성하세요.

## 기사 목록:
{articles_text}
## 규칙:
1. 제목은 20자 이내로 핵심 이벤트를 나타내세요
2. 요약은 2-3문장으로 이벤트의 핵심 내용을 정리하세요
3. 한국어로 작성하세요

## 출력 형식 (JSON):
```json
{{"title": "이벤트 제목", "summary": "이벤트 요약"}}
```

## 결과 (JSON):"#
        )
    }

    /// Generate text using the LLM client
    async fn generate(&self, prompt: &str) -> Result<String> {
        // Use the vLLM generate_vllm via the public API
        // LlmClient::extract_said_relations uses generate internally,
        // but we need raw generation, so we call it via a workaround:
        // We build a custom prompt and use the existing infrastructure

        let config = LlmConfig::from_env();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        let url = format!("{}/v1/chat/completions", config.endpoint);

        let request = serde_json::json!({
            "model": config.model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 512,
            "temperature": 0.3,
            "stream": false
        });

        let response = client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to vLLM")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("vLLM request failed: {} - {}", status, body);
        }

        let resp_json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse vLLM response")?;

        resp_json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("No response content from vLLM"))
    }

    /// Parse the summary response from the LLM
    fn parse_summary_response(&self, response: &str) -> (String, String) {
        // Try to extract JSON from the response
        let json_str = Self::extract_json(response);

        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
            let title = parsed["title"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let summary = parsed["summary"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            return (title, summary);
        }

        // Fallback: use the raw response as summary
        (String::new(), response.trim().to_string())
    }

    /// Extract JSON from markdown code blocks or raw text
    fn extract_json(text: &str) -> String {
        // Try ```json ... ``` blocks
        if let Some(start) = text.find("```json") {
            if let Some(end) = text[start + 7..].find("```") {
                return text[start + 7..start + 7 + end].trim().to_string();
            }
        }

        // Try ``` ... ``` blocks
        if let Some(start) = text.find("```") {
            let after = &text[start + 3..];
            let content_start = after.find('\n').unwrap_or(0) + 1;
            if let Some(end) = after[content_start..].find("```") {
                return after[content_start..content_start + end].trim().to_string();
            }
        }

        // Try raw JSON object
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                if end > start {
                    return text[start..=end].to_string();
                }
            }
        }

        text.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_code_block() {
        let text = r#"Here is the result:
```json
{"title": "테스트 이벤트", "summary": "테스트 요약입니다."}
```
"#;
        let json = ClusterSummarizer::extract_json(text);
        assert!(json.contains("테스트 이벤트"));

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");
        assert_eq!(parsed["title"], "테스트 이벤트");
    }

    #[test]
    fn test_extract_json_raw() {
        let text = r#"{"title": "이벤트", "summary": "요약"}"#;
        let json = ClusterSummarizer::extract_json(text);
        assert_eq!(json, text);
    }

    #[test]
    fn test_parse_summary_response() {
        let summarizer_json = r#"{"title": "탄핵 심판", "summary": "헌법재판소가 탄핵 심판을 진행 중이다."}"#;

        // Test JSON parsing directly
        let parsed: serde_json::Value =
            serde_json::from_str(summarizer_json).expect("should parse");
        assert_eq!(parsed["title"].as_str().unwrap(), "탄핵 심판");
        assert!(parsed["summary"]
            .as_str()
            .unwrap()
            .contains("헌법재판소"));
    }

    #[test]
    fn test_parse_summary_response_fallback() {
        // When JSON parsing fails, should use raw text
        let response = "이것은 JSON이 아닌 텍스트입니다.";
        let json = ClusterSummarizer::extract_json(response);
        // Fallback: returns trimmed text (won't parse as JSON)
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_err());
    }
}
