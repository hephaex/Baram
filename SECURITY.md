# Security Policy

## 보안 정책

Baram 프로젝트의 보안을 중요하게 생각합니다. 이 문서는 보안 취약점을 보고하는 방법과 보안 관련 정책을 설명합니다.

## 지원 버전

현재 보안 업데이트가 지원되는 버전:

| 버전 | 지원 상태 |
|------|----------|
| 0.1.x | ✅ 지원 |
| < 0.1.0 | ❌ 지원 종료 |

## 취약점 보고

### 보고 방법

**중요: 보안 취약점은 공개 이슈로 보고하지 마세요.**

보안 취약점을 발견하셨다면:

1. **이메일로 보고**: [hephaex@gmail.com](mailto:hephaex@gmail.com)
2. **제목**: `[SECURITY] Baram 보안 취약점 보고`
3. **내용에 포함할 정보**:
   - 취약점 유형 및 설명
   - 재현 단계
   - 영향 범위
   - 가능하다면 수정 제안

### 보고 템플릿

```
제목: [SECURITY] 취약점 유형 요약

## 취약점 설명
[취약점에 대한 상세 설명]

## 영향
- 영향 받는 구성 요소:
- 심각도 (Critical/High/Medium/Low):
- 잠재적 영향:

## 재현 단계
1. ...
2. ...
3. ...

## 환경
- Baram 버전:
- OS:
- 기타 관련 정보:

## 추가 정보
[스크린샷, 로그, PoC 코드 등]
```

### 응답 시간

- **확인 응답**: 48시간 이내
- **초기 평가**: 7일 이내
- **수정 계획**: 심각도에 따라 14-30일

### 공개 정책

1. 취약점 수정 후 패치 릴리스
2. 보안 권고 발행 (GitHub Security Advisories)
3. 보고자 크레딧 (원하는 경우)

## 보안 모범 사례

### 배포 시 체크리스트

Baram을 프로덕션에 배포할 때 다음을 확인하세요:

#### 1. 환경 변수 및 시크릿

```bash
# ❌ 하지 마세요
POSTGRES_PASSWORD=changeme

# ✅ 올바른 방법
POSTGRES_PASSWORD=$(openssl rand -base64 32)
```

- [ ] 모든 기본 비밀번호 변경
- [ ] 환경 변수로 시크릿 관리
- [ ] `.env` 파일 버전 관리 제외 확인
- [ ] 파일 권한 설정 (`chmod 600 .env`)

#### 2. 네트워크 보안

- [ ] TLS/SSL 활성화 (PostgreSQL, OpenSearch)
- [ ] 방화벽 규칙 설정
- [ ] 불필요한 포트 노출 제거
- [ ] 프라이빗 네트워크 사용

```yaml
# docker-compose.yml - 프로덕션 권장
services:
  postgres:
    ports:
      - "127.0.0.1:5432:5432"  # localhost만 바인딩
```

#### 3. 서비스 보안

**PostgreSQL:**
- [ ] 강력한 비밀번호 설정
- [ ] 불필요한 사용자 권한 제거
- [ ] SSL 연결 강제

**OpenSearch:**
- [ ] 보안 플러그인 활성화 (`DISABLE_SECURITY_PLUGIN=false`)
- [ ] 인증서 설정
- [ ] 역할 기반 접근 제어

**Redis:**
- [ ] 비밀번호 설정 (`requirepass`)
- [ ] 보호 모드 활성화

#### 4. 애플리케이션 보안

- [ ] Rate limiting 설정
- [ ] 로깅 활성화
- [ ] 에러 메시지에서 민감 정보 제거

### 알려진 보안 고려사항

#### 웹 크롤링

- 크롤링 대상 사이트의 `robots.txt` 준수
- Rate limiting으로 과도한 요청 방지
- User-Agent 적절히 설정

#### 데이터 저장

- 수집된 데이터의 적절한 접근 제어
- 개인정보 포함 가능성 있는 댓글 데이터 처리 주의
- 정기적인 데이터 백업

#### API 엔드포인트

- Coordinator API는 내부 네트워크에서만 접근하도록 설정
- 프로덕션에서 Health 엔드포인트 외부 노출 제한

## 의존성 보안

### 자동 보안 스캔

프로젝트는 다음 자동 보안 검사를 사용합니다:

```yaml
# .github/workflows/security.yml
- cargo-audit: Rust 의존성 취약점 검사
- Trivy: 컨테이너 이미지 취약점 검사
- dependency-review: PR 의존성 검토
```

### 수동 보안 검사

```bash
# Rust 의존성 보안 감사
cargo audit

# 의존성 업데이트 확인
cargo outdated

# 컨테이너 이미지 스캔
trivy image baram:latest
```

### 의존성 업데이트 정책

- **보안 패치**: 즉시 업데이트
- **마이너 업데이트**: 월간 검토
- **메이저 업데이트**: 분기별 검토 및 테스트

## 보안 연락처

- **이메일**: [hephaex@gmail.com](mailto:hephaex@gmail.com)
- **GitHub**: [@hephaex](https://github.com/hephaex)

## 감사의 말

보안 취약점을 책임감 있게 보고해 주신 분들께 감사드립니다.

<!-- 보안 기여자 목록은 허가를 받은 후 여기에 추가됩니다 -->

---

이 보안 정책은 정기적으로 검토되고 업데이트됩니다.

마지막 업데이트: 2024년 12월
