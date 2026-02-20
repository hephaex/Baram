# 뉴스 이벤트 감지 & 클러스터링

> 조사일: 2026-02-15
> 관련 Phase: 2 (이벤트 클러스터링), 5 (Temporal KG)

## LLM Enhanced Clustering

### 방법론
1. LLM으로 기사에서 키워드 추출
2. 키워드 임베딩 생성
3. 클러스터링 (HDBSCAN, cosine similarity)
4. LLM으로 클러스터 요약 + IPTC 토픽 카테고리 생성

### Time-Aware Embeddings
- 시간 가중치를 임베딩에 포함하여 최신성 반영
- Retrospective (과거 분석) + Online (실시간 감지) 두 모드 지원
- SOTA 성능 달성

## Temporal Knowledge Graph

### ECS-KG (Event-Centric Semantic KG)
- 딥러닝 + 컨텍스트 임베딩으로 동적 지식 표현
- Temporal GNN + Graph Attention Networks
- 뉴스 기사의 절차적/동적 지식 표현 개선

### Narrative Graph
- 이벤트 중심 KG에 시간 관계 추가
- GNN으로 이벤트 감지 + BERT로 시간 관계 식별
- 스토리라인 생성: coherence + coverage 제약 하에 내러티브 구성

### 시간축 관계 유형
```cypher
(:Event)-[:CAUSED]->(:Event)        # 인과관계
(:Event)-[:FOLLOWED_BY]->(:Event)   # 시간순
(:Event)-[:SIMILAR_TO]->(:Event)    # 유사 이벤트
(:Event {start_date, end_date, status})
```

## 국내 사례

### 인포시즈 GORAG
- 기업 문서 업로드만으로 온톨로지 구축
- RAG 대비 정확도: 60-70% → **90% 이상**
- 응답 속도: 0.5초대

### S2W 지식그래프
- NLP + 임베딩 유사도로 개체 간 관계 자동 추출
- 하이브리드 프로세스: AI 자동 매핑 + 도메인 전문가 검증
- 95% 이상 정밀 자동화 실현

## 참고 자료
- [LLM Enhanced Clustering for News (arXiv)](https://arxiv.org/abs/2406.10552)
- [Time-Aware Document Embeddings (arXiv)](https://arxiv.org/html/2112.06166v2)
- [ECS-KG (ScienceDirect)](https://www.sciencedirect.com/science/article/abs/pii/S0169023X25000461)
- [Temporal KG Generation (Nature)](https://www.nature.com/articles/s41597-025-05062-0)
- [Narrative Graph (Springer)](https://link.springer.com/article/10.1007/s11518-023-5561-0)
- [인포시즈 GORAG](https://infocz.co.kr/bbs/board.php?bo_table=s4_1&wr_id=1)
- [S2W 지식그래프](https://zdnet.co.kr/view/?no=20250328155440)
