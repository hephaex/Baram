//! Test LLM-based Said relation extraction

use anyhow::Result;
use baram::llm::LlmClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Sample Korean news article
    let article_text = r#"
윤석열 대통령은 28일 서울 용산 대통령실에서 기자회견을 열고 "국민의 뜻을 무겁게 받들겠다"고 밝혔다.

한덕수 국무총리도 같은 날 "정부는 국민과 소통하며 국정을 운영하겠다"고 강조했다.

이재명 더불어민주당 대표는 "현 정부의 정책에 대한 철저한 검증이 필요하다"며 "야당으로서 감시 역할을 다하겠다"고 말했다.

김기현 국민의힘 대표는 기자들에게 "여야 협력을 통해 민생 문제를 해결해 나가겠다"고 전했다.

경제 전문가 홍길동 박사는 "현재 경제 상황이 녹록지 않다"면서 "정부의 신속한 대응이 필요하다"고 분석했다.
"#;

    println!("=== LLM Said Relation Extraction Test ===\n");

    let client = LlmClient::new()?;

    // Check if Ollama is available
    if !client.is_available().await {
        eprintln!("Ollama is not available. Please start Ollama first.");
        return Ok(());
    }

    println!("Extracting Said relations from article...\n");

    match client.extract_said_relations(article_text).await {
        Ok(relations) => {
            println!("Found {} Said relations:\n", relations.len());

            for (i, rel) in relations.iter().enumerate() {
                println!("{}. Speaker: {}", i + 1, rel.speaker);
                println!("   Content: {}", rel.content);
                println!("   Confidence: {:.2}", rel.confidence);
                println!("   Evidence: {}", rel.evidence);
                println!();
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}
