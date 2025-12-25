# baram - n 뉴스 크롤러

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

> Rust 기반 고성능 n 뉴스 크롤러 + Vector DB + 온톨로지 시스템

## 개요

baram는 n 뉴스에서 기사와 댓글을 수집하여 벡터 데이터베이스에 저장하고, 온톨로지(지식 그래프)를 구축하는 시스템입니다.

### 주요 기능

- **뉴스 크롤링**: 정치, 경제, 사회, 문화, 세계, IT 카테고리 기사 수집
- **댓글 수집**: JSONP API를 통한 댓글 및 답글 재귀 수집
- **Vector DB**: OpenSearch + nori 분석기를 활용한 의미 기반 검색
- **온톨로지**: LLM 기반 관계 추출 및 지식 그래프 구축
- **이중 저장소**: SQLite(메타데이터) + PostgreSQL 18(원본 데이터)
- **분산 크롤링**: 다중 인스턴스 기반 시간 분할 크롤링
- **임베딩 서버**: GPU 가속 벡터 생성 API
- **모니터링**: Prometheus 메트릭 수집

## 시스템 요구사항

- Rust 1.75+
- Docker 24.0+
- PostgreSQL 18
- OpenSearch 3.4+ (nori 플러그인 포함)
- (선택) NVIDIA GPU + CUDA for GPU acceleration

## 빠른 시작

```bash
# 저장소 클론
git clone https://github.com/hephaex/baram.git
cd baram

# 의존성 설치 및 빌드
cargo build --release

# Docker 서비스 시작
cd docker
docker-compose up -d

# 크롤링 실행
cargo run -- crawl --category politics --max-articles 100

# 검색
cargo run -- search "반도체 투자"
```

## 프로젝트 구조

```
baram/
├── src/
│   ├── crawler/       # HTTP Fetcher, 댓글 크롤러, 분산 크롤러
│   ├── coordinator/   # 분산 크롤링 코디네이터 서버
│   ├── parser/        # HTML 파서
│   ├── storage/       # SQLite, PostgreSQL, Markdown
│   ├── embedding/     # 토크나이저, 벡터화
│   ├── metrics/       # Prometheus 메트릭
│   └── ontology/      # 관계 추출, Entity Linking
├── tests/
│   └── fixtures/      # 테스트용 HTML, JSONP 샘플
├── docs/
│   └── *.md           # 개발 문서
└── docker/
    ├── docker-compose.yml              # 기본 서비스
    ├── docker-compose.distributed.yml  # 분산 크롤링
    └── docker-compose.gpu.yml          # GPU 가속
```

## CLI 명령어

### 기본 크롤링

```bash
# 카테고리별 크롤링
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

### 분산 크롤링 모드

분산 크롤러는 여러 인스턴스가 시간대별로 크롤링 작업을 나누어 수행합니다.

```bash
# 분산 크롤러 실행
baram distributed \
    --instance main \
    --coordinator http://localhost:8080 \
    --database "postgresql://user:pass@localhost:5432/baram" \
    --rps 2.0 \
    --output ./output \
    --with-comments
```

**주요 옵션:**

| 옵션 | 설명 | 기본값 |
|------|------|--------|
| `--instance` | 인스턴스 ID (main, sub1, sub2) | - |
| `--coordinator` | 코디네이터 서버 URL | http://localhost:8080 |
| `--database` | PostgreSQL URL (중복 제거용) | - |
| `--heartbeat-interval` | 하트비트 전송 주기 (초) | 30 |
| `--rps` | 초당 요청 수 | 1.0 |
| `--output` | 출력 디렉토리 | ./output |
| `--with-comments` | 댓글 수집 여부 | true |
| `--once` | 현재 슬롯만 실행 후 종료 | false |

### 코디네이터 서비스

코디네이터는 분산 크롤러 인스턴스들의 스케줄을 관리하고 상태를 모니터링합니다.

```bash
# 코디네이터 서버 시작
baram coordinator \
    --port 8080 \
    --host 0.0.0.0 \
    --heartbeat-timeout 90 \
    --max-instances 10
```

**API 엔드포인트:**

| 엔드포인트 | 메서드 | 설명 |
|------------|--------|------|
| `/api/health` | GET | 헬스 체크 |
| `/api/instances` | GET | 등록된 인스턴스 목록 |
| `/api/instances/:id` | GET | 특정 인스턴스 정보 |
| `/api/instances/register` | POST | 인스턴스 등록 |
| `/api/instances/heartbeat` | POST | 하트비트 전송 |
| `/api/schedule/today` | GET | 오늘의 스케줄 |
| `/api/schedule/tomorrow` | GET | 내일의 스케줄 |
| `/api/schedule/:date` | GET | 특정 날짜 스케줄 (YYYY-MM-DD) |
| `/api/stats` | GET | 코디네이터 통계 |
| `/metrics` | GET | Prometheus 메트릭 |

### 임베딩 서버

임베딩 서버는 텍스트를 벡터로 변환하는 REST API를 제공합니다.

```bash
# 임베딩 서버 시작
baram embedding-server \
    --port 8090 \
    --host 0.0.0.0 \
    --model intfloat/multilingual-e5-large \
    --max-seq-length 512 \
    --batch-size 32 \
    --use-gpu
```

**API 엔드포인트:**

| 엔드포인트 | 메서드 | 설명 |
|------------|--------|------|
| `/health` | GET | 헬스 체크 |
| `/embed` | POST | 단일 텍스트 임베딩 |
| `/embed/batch` | POST | 배치 텍스트 임베딩 (최대 100개) |

**사용 예시:**

```bash
# 단일 텍스트 임베딩
curl -X POST http://localhost:8090/embed \
    -H "Content-Type: application/json" \
    -d '{"text": "반도체 산업 동향"}'

# 배치 임베딩
curl -X POST http://localhost:8090/embed/batch \
    -H "Content-Type: application/json" \
    -d '{"texts": ["텍스트1", "텍스트2", "텍스트3"]}'
```

## Prometheus 메트릭

코디네이터와 크롤러 모두 `/metrics` 엔드포인트를 통해 Prometheus 형식의 메트릭을 제공합니다.

### 코디네이터 메트릭

| 메트릭 | 타입 | 설명 |
|--------|------|------|
| `baram_coordinator_registered_instances` | Gauge | 등록된 인스턴스 수 |
| `baram_coordinator_online_instances` | Gauge | 온라인 인스턴스 수 |
| `baram_coordinator_total_heartbeats` | Counter | 총 하트비트 수 |
| `baram_coordinator_heartbeat_errors_total` | Counter | 하트비트 오류 수 |
| `baram_coordinator_articles_crawled_total` | Counter | 인스턴스별 크롤링 기사 수 |
| `baram_coordinator_errors_total` | Counter | 인스턴스별 오류 수 |
| `baram_coordinator_api_requests_total` | Counter | API 요청 수 (엔드포인트, 상태별) |
| `baram_coordinator_api_request_duration_seconds` | Histogram | API 요청 응답 시간 |

### 크롤러 메트릭

| 메트릭 | 타입 | 설명 |
|--------|------|------|
| `baram_crawler_crawl_duration_seconds` | Histogram | 카테고리별 크롤링 시간 |
| `baram_crawler_articles_per_category_total` | Counter | 카테고리별 크롤링 기사 수 |
| `baram_crawler_dedup_hits_total` | Counter | 중복 URL 수 |
| `baram_crawler_dedup_misses_total` | Counter | 새로운 URL 수 |
| `baram_crawler_pipeline_success_total` | Counter | 파이프라인 성공 수 |
| `baram_crawler_pipeline_failure_total` | Counter | 파이프라인 실패 수 |
| `baram_crawler_slot_executions_total` | Counter | 슬롯 실행 횟수 |
| `baram_crawler_is_crawling` | Gauge | 현재 크롤링 중 (1/0) |
| `baram_crawler_current_hour` | Gauge | 현재 크롤링 시간대 |

## Docker 배포

### 기본 서비스 배포

기본 docker-compose.yml은 PostgreSQL, OpenSearch, Redis를 포함합니다.

```bash
cd docker

# .env 파일 생성
cp .env.example .env
# POSTGRES_PASSWORD 등 필수 환경변수 설정

# 기본 서비스 시작
docker-compose up -d

# 개발용 도구 (pgAdmin, OpenSearch Dashboards) 포함
docker-compose --profile development up -d
```

### 분산 크롤링 배포

분산 크롤링은 코디네이터와 3개의 크롤러 인스턴스를 배포합니다.

```bash
# 기본 서비스 + 분산 크롤링
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml up -d
```

**배포되는 서비스:**

| 서비스 | 컨테이너명 | 포트 | 설명 |
|--------|------------|------|------|
| coordinator | baram-coordinator | 8080 | 스케줄 관리 서버 |
| crawler-main | baram-crawler-main | - | 메인 크롤러 (ID: main) |
| crawler-sub1 | baram-crawler-sub1 | - | 서브 크롤러 1 (ID: sub1) |
| crawler-sub2 | baram-crawler-sub2 | - | 서브 크롤러 2 (ID: sub2) |

### GPU 가속 배포

GPU를 사용하여 임베딩 생성 속도를 향상시킬 수 있습니다.

**요구사항:**
- NVIDIA GPU with CUDA support
- nvidia-container-toolkit 설치
- Docker 19.03+

```bash
# 기본 서비스 + GPU 서비스
docker-compose -f docker-compose.yml -f docker-compose.gpu.yml up -d
```

**배포되는 GPU 서비스:**

| 서비스 | 컨테이너명 | 포트 | 설명 |
|--------|------------|------|------|
| crawler-gpu | baram-crawler-gpu | - | GPU 가속 크롤러 |
| embedding-service | baram-embedding-gpu | 8090 | GPU 임베딩 서버 |

### 환경 변수

주요 환경 변수 (docker/.env):

```bash
# PostgreSQL
POSTGRES_DB=baram
POSTGRES_USER=baram
POSTGRES_PASSWORD=<your-password>
POSTGRES_PORT=5432

# OpenSearch
OPENSEARCH_PORT=9200

# Redis
REDIS_PORT=6379
REDIS_MAXMEMORY=256mb

# Coordinator
COORDINATOR_PORT=8080
HEARTBEAT_TIMEOUT=90
HEARTBEAT_INTERVAL=30
MAX_INSTANCES=10

# Crawler
REQUESTS_PER_SECOND=2.0
CRAWLER_LOG_LEVEL=info
COORDINATOR_LOG_LEVEL=info

# GPU Embedding
EMBEDDING_PORT=8090
EMBEDDING_MODEL=intfloat/multilingual-e5-large
EMBEDDING_BATCH_SIZE=32
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
database = "baram"

[opensearch]
hosts = ["http://localhost:9200"]
index_name = "naver-news"
```

## 라이센스

이 프로젝트는 [GPL v3 라이센스](LICENSE)를 따릅니다.

## 저작권

Copyright (c) 2025 hephaex@gmail.com

## 기여

기여를 환영합니다! [이슈](https://github.com/hephaex/baram/issues)를 통해 버그 리포트나 기능 제안을 해주세요.
