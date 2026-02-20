# References Index

> 에이전트가 관련 자료를 찾을 때 이 인덱스를 참조합니다.
> 새 자료 추가 시 이 파일도 업데이트해야 합니다.

## 파일 목록

| 파일 | 관련 Phase | 핵심 주제 |
|------|-----------|----------|
| `graphrag-knowledge-graph.md` | Phase 3 | GraphRAG, Neo4j, 온톨로지 그라운딩, KARMA, AutoSchemaKG |
| `rag-vector-search-best-practices.md` | Phase 1, 4 | Hybrid Search, BM25+kNN, Semantic Chunking, Cross-Encoder Reranking |
| `event-detection-clustering.md` | Phase 2, 5 | LLM 클러스터링, Temporal KG, ECS-KG, Narrative Graph, GORAG |
| `news-crawling-architecture.md` | 전체 | StormCrawler, news-please, Nutch, Scrapy, 아키텍처 비교 |
| `opensearch-hybrid-search.md` | Phase 1 | OpenSearch hybrid query, normalization-processor, search_pipeline, opensearch-rs 구현 |

## 키워드 → 파일 매핑

- **GraphRAG, Neo4j, 지식그래프, 온톨로지** → `graphrag-knowledge-graph.md`
- **Hybrid Search, BM25, 하이브리드 검색** → `rag-vector-search-best-practices.md`
- **Chunking, Reranking, Cross-Encoder, 임베딩** → `rag-vector-search-best-practices.md`
- **이벤트 감지, 클러스터링, 토픽** → `event-detection-clustering.md`
- **Temporal KG, 시간축, 타임라인** → `event-detection-clustering.md`
- **크롤링, 아키텍처, StormCrawler** → `news-crawling-architecture.md`
- **GORAG, S2W, 국내 사례** → `event-detection-clustering.md`
- **OpenSearch hybrid query, normalization-processor, search_pipeline, hybrid-pipeline** → `opensearch-hybrid-search.md`
- **opensearch-rs search_pipeline 구현, cosineSimilarity 오류, OpenSearch 3.x** → `opensearch-hybrid-search.md`

## 새 자료 추가 규칙

1. `.claude/references/` 에 마크다운 파일 생성
2. 파일 상단에 메타 정보 포함 (조사일, 관련 Phase)
3. `INDEX.md`에 파일 추가 (파일 목록 + 키워드 매핑)
4. 관련 Phase의 PLAN.md에서 참조 링크 추가
