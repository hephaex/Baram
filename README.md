# nTimes - 네이버 뉴스 크롤러

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

> Rust 기반 고성능 네이버 뉴스 크롤러 + Vector DB + 온톨로지 시스템

## 개요

nTimes는 네이버 뉴스에서 기사와 댓글을 수집하여 벡터 데이터베이스에 저장하고, 온톨로지(지식 그래프)를 구축하는 시스템입니다.

### 주요 기능

- **뉴스 크롤링**: 정치, 경제, 사회, 문화, 세계, IT 카테고리 기사 수집
- **댓글 수집**: JSONP API를 통한 댓글 및 답글 재귀 수집
- **Vector DB**: OpenSearch + nori 분석기를 활용한 의미 기반 검색
- **온톨로지**: LLM 기반 관계 추출 및 지식 그래프 구축
- **이중 저장소**: SQLite(메타데이터) + PostgreSQL 18(원본 데이터)

## 시스템 요구사항

- Rust 1.75+
- Docker 24.0+
- PostgreSQL 18
- OpenSearch 2.11+ (nori 플러그인 포함)

## 빠른 시작

```bash
# 저장소 클론
git clone https://github.com/hephaex/nTimes.git
cd nTimes

# 의존성 설치 및 빌드
cargo build --release

# Docker 서비스 시작
docker-compose up -d

# 크롤링 실행
cargo run -- crawl --category politics --max-articles 100

# 검색
cargo run -- search "반도체 투자"
```

## 프로젝트 구조

```
nTimes/
├── src/
│   ├── crawler/       # HTTP Fetcher, 댓글 크롤러
│   ├── parser/        # HTML 파서
│   ├── storage/       # SQLite, PostgreSQL, Markdown
│   ├── embedding/     # 토크나이저, 벡터화
│   └── ontology/      # 관계 추출, Entity Linking
├── tests/
│   └── fixtures/      # 테스트용 HTML, JSONP 샘플
├── docs/
│   └── *.md           # 개발 문서
└── docker/
    └── docker-compose.yml
```

## CLI 명령어

```bash
# 크롤링
cargo run -- crawl --category <카테고리> --max-articles <개수>
cargo run -- crawl --url <URL> --with-comments

# 인덱싱
cargo run -- index --input ./output/raw --batch-size 100

# 검색
cargo run -- search "검색어" --k 10

# 온톨로지 추출
cargo run -- ontology --input ./output/raw --format json

# 재개
cargo run -- resume --checkpoint ./checkpoints/crawl_state.json
```

## 설정

`config.toml` 파일을 통해 설정을 관리합니다:

```toml
[crawler]
requests_per_second = 2
max_retries = 3

[postgresql]
host = "localhost"
port = 5432
database = "ntimes"

[opensearch]
hosts = ["http://localhost:9200"]
index_name = "naver-news"
```

## 라이센스

이 프로젝트는 [GPL v3 라이센스](LICENSE)를 따릅니다.

## 저작권

Copyright (c) 2024 hephaex@gmail.com

## 기여

기여를 환영합니다! [이슈](https://github.com/hephaex/nTimes/issues)를 통해 버그 리포트나 기능 제안을 해주세요.
