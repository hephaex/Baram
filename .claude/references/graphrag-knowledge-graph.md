# GraphRAG & Knowledge Graph

> 조사일: 2026-02-15
> 관련 Phase: 3 (GraphRAG + Neo4j)

## 핵심 개념

GraphRAG는 벡터 검색 + 지식그래프 탐색을 결합한 하이브리드 검색 아키텍처.

### 동작 방식
1. 쿼리 수신 → 임베딩 생성 + 엔티티 링킹 동시 수행
2. 벡터 검색 채널: 텍스트 패시지 검색
3. 그래프 검색 채널: 관련 서브그래프, 이웃 노드, multi-hop 관계 추출
4. Context Merger: 텍스트 + 구조화 데이터를 LLM 프롬프트에 결합
5. Reciprocal Rank Fusion으로 두 결과 병합

### 성능
- Hybrid indexing (dense + BM25 sparse): **15-30% 정밀도 향상**
- 사실 정확도에서 특히 큰 차이
- 환각 감소: 온톨로지 기반 "Truth Layer"

### 2025-2026 주요 동향
- **온톨로지 그라운딩**: LLM이 추론 시 온톨로지 참조 (Palantir, Microsoft 등)
- **AutoSchemaKG**: 스키마 기반 + 스키마 프리 통합, 실시간 온톨로지 진화
- **KARMA 프레임워크**: 다중 에이전트 (스키마 정렬, 충돌 해결, 품질 평가)
- **그래프 DB 시장**: $2.85B (2025) → $15.32B (2032), CAGR 27.1%

### 구현 도구
- **Microsoft GraphRAG**: https://microsoft.github.io/graphrag/
- **Neo4j LLM Graph Builder**: https://neo4j.com/labs/genai-ecosystem/llm-graph-builder/
- **LangChain LLMGraphTransformer**: 비정형 텍스트 → 지식그래프 자동 변환

## 참고 자료
- [GraphRAG & Knowledge Graphs (Fluree)](https://flur.ee/fluree-blog/graphrag-knowledge-graphs-making-your-data-ai-ready-for-2026/)
- [Practical GraphRAG at Scale (arXiv)](https://arxiv.org/abs/2507.03226)
- [Microsoft GraphRAG](https://microsoft.github.io/graphrag/)
- [Neo4j GraphRAG Patterns](https://neo4j.com/nodes-2025/agenda/enhancing-retrieval-augmented-generation-with-graphrag-patterns-in-neo4j/)
- [LLM-empowered KG Construction Survey](https://arxiv.org/html/2510.20345v1)
- [GraphRAG Complete Guide (Meilisearch)](https://www.meilisearch.com/blog/graph-rag)
