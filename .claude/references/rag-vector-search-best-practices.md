# RAG & Vector Search Best Practices

> 조사일: 2026-02-15
> 관련 Phase: 1 (Hybrid Search), 4 (Semantic Chunking + Reranking)

## Hybrid Search (BM25 + kNN)

### 원리
- BM25: 키워드 정확 매칭 (TF-IDF 기반)
- kNN: 의미적 유사도 (dense vector)
- 결합: Reciprocal Rank Fusion 또는 가중 평균
- **정밀도 15-30% 향상** (enterprise 배포 기준)

### OpenSearch 구현
```json
PUT /_search/pipeline/hybrid-pipeline
{
  "phase_results_processors": [{
    "normalization-processor": {
      "normalization": { "technique": "min_max" },
      "combination": { "technique": "arithmetic_mean", "parameters": { "weights": [0.3, 0.7] } }
    }
  }]
}
```

## Semantic Chunking

### 최적 설정
- 청크 크기: **256-512 토큰** (100-200 단어)
- 오버랩: 10-20%
- Semantic chunking이 fixed-size 대비 **recall 9% 향상**
- 각 청크에 문서 메타데이터 (제목, 카테고리) 주입 필수

### 임베딩 모델 선택
- Voyage-3-large: OpenAI/Cohere 대비 9-20% 성능 향상, 32K context
- 도메인 특화 모델: 범용 대비 20-40% 정확도 향상
- 한국어: multilingual 모델 사용 필수

## Cross-Encoder Reranking

### 원리
- 1차 검색 (bi-encoder): 빠르지만 덜 정확
- 2차 리랭킹 (cross-encoder): 느리지만 더 정확 (query-document 쌍 직접 스코어링)
- **정밀도 10-30% 추가 향상**, 지연 50-100ms (50 candidates)

### 추천 모델
- `BAAI/bge-reranker-v2-m3` (다국어)
- Cohere Rerank
- BGE Reranker

### 파이프라인
```
query → hybrid search → top-50 → cross-encoder rerank → top-10
```

## 참고 자료
- [RAG Implementation Guide](https://www.mayhemcode.com/2025/12/rag-implementation-guide-embedding.html)
- [Cross-Encoder Reranking (OpenAI Cookbook)](https://cookbook.openai.com/examples/search_reranking_with_cross-encoders)
- [Semantic Reranking (Elastic)](https://www.elastic.co/docs/solutions/search/ranking/semantic-reranking)
- [RAG Best Practices Study (arXiv)](https://arxiv.org/abs/2501.07391)
- [Microsoft RAG Techniques](https://www.microsoft.com/en-us/microsoft-cloud/blog/2025/02/04/common-retrieval-augmented-generation-rag-techniques-explained/)
