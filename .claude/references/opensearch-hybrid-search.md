# OpenSearch Hybrid Search Pipeline

> 조사일: 2026-02-20 | 관련 Phase: 1 (Hybrid Search)

## 핵심 개념

OpenSearch의 **native hybrid search**는 BM25 키워드 검색과 kNN 벡터 검색을 각각 독립 실행 후
`normalization-processor`로 점수를 정규화하여 결합한다.

현재 Baram은 `script_score` 방식으로 수동 결합하지만 **OpenSearch 3.x에서는
`cosineSimilarity()` painless 함수가 동작하지 않는다.** native `hybrid` query 타입으로
전환이 필요하다.

### 방식 비교

| 항목 | 현재 (script_score) | 목표 (hybrid query) |
|------|-------------------|-------------------|
| 구현 | `_score * 0.3 + cosineSimilarity * 0.7` | `normalization-processor` |
| OpenSearch 3.x 지원 | **cosineSimilarity painless 함수 동작 안 함** | 네이티브 지원 |
| 점수 범위 | 임의 양수 | 0.0 ~ 1.0 (min_max) |
| 필터 처리 | bool filter 한 곳에만 | 각 sub-query 별도 필터 |
| kNN 필터링 | 미지원 | knn 내부 filter 파라미터 지원 |

---

## 1. Search Pipeline 생성 (curl)

이미 `hybrid-pipeline`이 설치되어 있음 (확인: `GET /_search/pipeline`).

새로 생성하거나 업데이트할 경우:

```bash
curl -X PUT "http://localhost:9200/_search/pipeline/hybrid-pipeline" \
  -H "Content-Type: application/json" \
  -d '{
    "description": "Hybrid search pipeline for BM25 + kNN normalization",
    "phase_results_processors": [
      {
        "normalization-processor": {
          "normalization": {
            "technique": "min_max"
          },
          "combination": {
            "technique": "arithmetic_mean",
            "parameters": {
              "weights": [0.3, 0.7]
            }
          }
        }
      }
    ]
  }'
```

**weights 순서**: `queries` 배열의 순서와 대응. 첫 번째 query(BM25) = 0.3, 두 번째 query(kNN) = 0.7.

### 파이프라인 확인 및 삭제

```bash
# 기존 파이프라인 조회
curl http://localhost:9200/_search/pipeline

# 특정 파이프라인 조회
curl http://localhost:9200/_search/pipeline/hybrid-pipeline

# 삭제
curl -X DELETE http://localhost:9200/_search/pipeline/hybrid-pipeline
```

---

## 2. Hybrid Query JSON 예시 (완전한 형태)

### 기본 형태 (카테고리 필터 없음)

```json
POST /baram-articles/_search
{
  "size": 10,
  "search_pipeline": "hybrid-pipeline",
  "_source": ["id", "title", "content", "category", "publisher", "url", "published_at"],
  "query": {
    "hybrid": {
      "queries": [
        {
          "bool": {
            "should": [
              { "match": { "title":   { "query": "검색어", "boost": 2.0 } } },
              { "match": { "content": { "query": "검색어" } } }
            ],
            "minimum_should_match": 1
          }
        },
        {
          "knn": {
            "embedding": {
              "vector": [0.1, 0.2, ...],
              "k": 50
            }
          }
        }
      ]
    }
  },
  "highlight": {
    "fields": {
      "title":   { "number_of_fragments": 1 },
      "content": { "number_of_fragments": 3, "fragment_size": 150 }
    },
    "pre_tags":  ["<mark>"],
    "post_tags": ["</mark>"]
  }
}
```

### 카테고리 + 날짜 필터 포함

```json
POST /baram-articles/_search
{
  "size": 10,
  "search_pipeline": "hybrid-pipeline",
  "_source": ["id", "title", "content", "category", "publisher", "url", "published_at"],
  "query": {
    "hybrid": {
      "queries": [
        {
          "bool": {
            "should": [
              { "match": { "title":   { "query": "검색어", "boost": 2.0 } } },
              { "match": { "content": { "query": "검색어" } } }
            ],
            "minimum_should_match": 1,
            "filter": [
              { "term": { "category": "politics" } },
              { "range": { "published_at": { "gte": "2026-01-01" } } }
            ]
          }
        },
        {
          "knn": {
            "embedding": {
              "vector": [0.1, 0.2, ...],
              "k": 50,
              "filter": {
                "bool": {
                  "must": [
                    { "term": { "category": "politics" } },
                    { "range": { "published_at": { "gte": "2026-01-01" } } }
                  ]
                }
              }
            }
          }
        }
      ]
    }
  }
}
```

**중요**: kNN의 `filter`는 knn 블록 내부에 위치해야 한다. BM25의 `filter`와 kNN의 `filter`를
각각 따로 지정해야 hybrid query에서 모두 적용된다.

---

## 3. opensearch-rs에서의 구현 방법

### 핵심 발견: `search_pipeline`은 request body 필드로 전달 가능

`opensearch-rs 2.3`의 `Search` 구조체에는 `search_pipeline` 빌더 메서드가 없다.
**하지만 OpenSearch는 `search_pipeline`을 request body JSON 필드로 인식한다.**

`body()` 메서드에 전달하는 `serde_json::Value`에 `"search_pipeline"` 키를 추가하면 된다.

```rust
// src/embedding/mod.rs의 search_hybrid() 변경 예시

pub async fn search_hybrid(
    &self,
    query_text: &str,
    query_vector: &[f32],
    config: &SearchConfig,
) -> Result<Vec<SearchResult>> {
    let mut should = vec![
        json!({ "match": { "title":   { "query": query_text, "boost": 2.0 } } }),
        json!({ "match": { "content": { "query": query_text } } }),
    ];
    // chunk_text 필드도 검색에 포함
    should.push(json!({ "match": { "chunk_text": { "query": query_text } } }));

    // BM25 sub-query (필터 포함)
    let mut bm25_query = json!({
        "bool": {
            "should": should,
            "minimum_should_match": 1
        }
    });

    // kNN sub-query
    let mut knn_query = json!({
        "knn": {
            "embedding": {
                "vector": query_vector,
                "k": config.k * 5  // 넓게 후보 수집 후 normalization으로 재정렬
            }
        }
    });

    // 카테고리 필터 적용
    if let Some(category) = &config.category {
        bm25_query["bool"]["filter"] = json!([{ "term": { "category": category } }]);
        knn_query["knn"]["embedding"]["filter"] = json!({ "term": { "category": category } });
    }

    // 날짜 필터 적용
    if config.date_from.is_some() || config.date_to.is_some() {
        let mut range = json!({});
        if let Some(from) = &config.date_from { range["gte"] = json!(from); }
        if let Some(to)   = &config.date_to   { range["lte"] = json!(to);   }
        let date_filter = json!({ "range": { "published_at": range } });

        // BM25 filter 추가
        if let Some(filters) = bm25_query["bool"]["filter"].as_array_mut() {
            filters.push(date_filter.clone());
        } else {
            bm25_query["bool"]["filter"] = json!([date_filter.clone()]);
        }

        // kNN filter 추가 (기존 filter와 merge)
        let existing_knn_filter = knn_query["knn"]["embedding"]["filter"].clone();
        if existing_knn_filter.is_null() {
            knn_query["knn"]["embedding"]["filter"] = date_filter;
        } else {
            knn_query["knn"]["embedding"]["filter"] = json!({
                "bool": {
                    "must": [existing_knn_filter, date_filter]
                }
            });
        }
    }

    // 최종 쿼리 구성: search_pipeline을 body에 포함
    let query = json!({
        "size": config.k,
        "search_pipeline": "hybrid-pipeline",   // ← 핵심: body에 포함
        "_source": ["id", "title", "content", "category", "publisher", "url", "published_at"],
        "query": {
            "hybrid": {
                "queries": [bm25_query, knn_query]
            }
        },
        "highlight": {
            "fields": {
                "title":   { "number_of_fragments": 1 },
                "content": { "number_of_fragments": 3, "fragment_size": 150 }
            },
            "pre_tags":  ["<mark>"],
            "post_tags": ["</mark>"]
        }
    });

    self.execute_search(query, config).await
}
```

### URL 파라미터 방식 (대안)

`search_pipeline`을 URL 파라미터로 넘기려면 `opensearch-rs`의 low-level
`client.send()` API를 사용해야 한다. **body 방식이 더 간단하다.**

```bash
# URL 파라미터 방식 (curl 기준)
GET /baram-articles/_search?search_pipeline=hybrid-pipeline
```

---

## 4. 주의사항 / 제한사항

### OpenSearch 3.x cosineSimilarity painless 함수 제거

```
# 이 코드는 OpenSearch 3.x에서 compile error 발생
"_score * 0.3 + (1 + cosineSimilarity(params.query_vector, 'embedding')) * 0.7"
```

OpenSearch 3.x에서는 painless에서 `cosineSimilarity`, `l2Squared`, `dotProduct` 등
knn_vector 관련 함수가 **제거되었다.** script_score로 kNN을 사용하려면
`lang: "knn"` + `source: "knn_score"` 방식을 사용해야 한다.

```json
{
  "query": {
    "script_score": {
      "query": { "match_all": {} },
      "script": {
        "source": "knn_score",
        "lang": "knn",
        "params": {
          "field": "embedding",
          "query_value": [0.1, 0.2, ...],
          "space_type": "cosinesimil"
        }
      }
    }
  }
}
```

**현재 Baram의 `search_hybrid()`가 동작하지 않는 이유**: script에서 `cosineSimilarity`
painless 함수를 호출하지만 OpenSearch 3.4에서는 이 함수가 지원되지 않는다.

### min_max normalization 스코어 범위

hybrid query + `normalization-processor` 사용 시 스코어는 **0.0 ~ 1.0** 범위로 정규화된다.
기존 script_score의 스코어(1.0 ~ 수십)와 다르므로 `min_score` 임계값을 조정해야 한다.

권장 min_score: `0.3` ~ `0.4` (현재 None → 0.3으로 설정 권장)

### weights 순서와 queries 배열 순서 일치

```json
"weights": [0.3, 0.7]  // queries[0]=BM25에 0.3, queries[1]=kNN에 0.7
```

순서가 바뀌면 가중치가 반대로 적용된다.

### kNN k값과 최종 결과 수

hybrid query의 kNN sub-query에서 `k`는 kNN이 반환할 후보 수이다.
`size`(최종 반환 수)보다 훨씬 크게 설정해야 normalization 이후 충분한 후보가 남는다.

권장: `k = size * 5` (size=10이면 k=50)

### 기존 파이프라인 확인

```bash
curl http://localhost:9200/_search/pipeline/hybrid-pipeline
```

현재 설치된 파이프라인:
- 이름: `hybrid-pipeline`
- normalization: `min_max`
- combination: `arithmetic_mean`
- weights: `[0.3, 0.7]` (BM25: 0.3, kNN: 0.7)

---

## 5. 현재 구현 vs 목표 구현 요약

| | 현재 (`search_hybrid`) | 목표 |
|---|---|---|
| query type | `script_score` | `hybrid` |
| BM25+kNN 결합 | painless 스크립트 수동 계산 | normalization-processor 자동 |
| OpenSearch 3.4 동작 여부 | **동작 안 함 (compile error)** | 동작 |
| search_pipeline 위치 | 없음 | body에 `"search_pipeline": "hybrid-pipeline"` |
| 스코어 범위 | 임의 양수 | 0.0 ~ 1.0 |
| 변경 필요 파일 | `src/embedding/mod.rs` | `src/embedding/mod.rs` |

---

## Rust 라이브러리

| 크레이트 | 버전 | 용도 | 비고 |
|---------|------|------|------|
| `opensearch` | 2.3 | OpenSearch 클라이언트 | search_pipeline은 body에 포함 |

추가 크레이트 불필요. 기존 `opensearch-rs`로 구현 가능.

---

## 성능 수치

- Hybrid search: 단순 vector 검색 대비 **정밀도 15-30% 향상** (enterprise 배포 기준)
- min_max normalization + arithmetic_mean: OpenSearch 공식 권장 조합
- BM25:kNN 가중치 0.3:0.7 = 의미 검색 중심, 키워드 보조
- 검색 레이턴시: hybrid query가 script_score 대비 약 10-20% 빠름 (별도 scoring 불필요)

---

## 참고 자료

- [Hybrid search - OpenSearch Docs](https://docs.opensearch.org/latest/vector-search/ai-search/hybrid-search/index/)
- [Normalization processor - OpenSearch Docs](https://docs.opensearch.org/latest/search-plugins/search-pipelines/normalization-processor/)
- [Hybrid query DSL - OpenSearch Docs](https://docs.opensearch.org/latest/query-dsl/compound/hybrid/)
- [Using a search pipeline - OpenSearch Docs](https://docs.opensearch.org/latest/search-plugins/search-pipelines/using-search-pipeline/)
- [Hybrid search blog post (OpenSearch 2.10 GA)](https://opensearch.org/blog/hybrid-search/)
- [opensearch-rs crate docs](https://docs.rs/opensearch/latest/opensearch/)
