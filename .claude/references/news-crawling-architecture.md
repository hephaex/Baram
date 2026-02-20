# 뉴스 크롤링 아키텍처

> 조사일: 2026-02-15
> 관련 Phase: 전체 (인프라)

## 오픈소스 크롤링 프레임워크

### Apache StormCrawler (Top-Level Project, 2025)
- 스트림 기반 분산 크롤러 (Apache Storm)
- URL이 DAG를 통해 fetch → parse → store로 흐름
- 배치가 아닌 연속 처리 방식
- CommonCrawl News Crawl이 StormCrawler 기반

### news-please
- 뉴스 전용 오픈소스 크롤러
- RSS + 내부 링크 재귀 탐색
- ElasticSearch 통합 + 버전 관리 지원
- 구조화된 뉴스 데이터 자동 추출

### Apache Nutch
- 대규모 웹 크롤링 + 인덱싱
- Hadoop 기반 분산 처리

### Scrapy
- Python 가장 인기 있는 크롤링 프레임워크
- JSON, XML, CSV 출력 지원

## Baram과의 비교

| 기능 | Baram (현재) | StormCrawler | news-please |
|------|-------------|--------------|-------------|
| 언어 | Rust | Java | Python |
| 분산 | 단일 서버 | Apache Storm | 단일 서버 |
| 크롤링 방식 | API + HTML | RSS/Sitemap | RSS + Recursive |
| 인덱싱 | OpenSearch | Elastic | Elastic |
| 중복 제거 | 3-tier (bloom+hash+DB) | URL filter | URL dedup |
| 벡터 검색 | kNN 384-dim | N/A | N/A |
| 온톨로지 | LLM 트리플 추출 | N/A | N/A |

Baram의 차별점: LLM 기반 온톨로지 + 벡터 검색 통합

## 참고 자료
- [Common Crawl News Crawl](https://commoncrawl.org/news-crawl)
- [news-please (GitHub)](https://github.com/fhamborg/news-please)
- [Azure Crawling Pipeline](https://techcommunity.microsoft.com/blog/azurestorageblog/building-a-scalable-web-crawling-and-indexing-pipeline-with-azure-storage-and-ai/4295042)
- [Open Source Crawlers Compared (pxe.gr)](https://pxe.gr/en/search-engines/open-source-crawler-frameworks-compared)
