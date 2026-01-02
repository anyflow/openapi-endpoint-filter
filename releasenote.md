# Release Notes

## [0.3.2] - 2026-01-02

- **Breaking Changes**: 캐시 기능 제거 (`cacheSize` 설정 무시)
- **Migration**: 설정에서 `cacheSize` 제거
- **Commit**: 26c71e3

### 1. Cache Feature Removed

경로 매칭 캐시(LRU) 기능을 제거하여 단순화.

**주요 변경사항**:

1. **캐시 설정 제거**
   - `cacheSize` 설정 삭제
   - 캐시 활성/비활성 옵션 제거

2. **의존성 정리**
   - `lru` 크레이트 제거

3. **로그 메시지 단순화**
   - 캐시 히트/미스 로그 제거
   - 라우터 매칭 로그만 유지

**영향**:
- ✅ 구성 단순화 및 의존성 감소
- ✅ 캐시 관련 테스트/설정 제거

## [0.3.1] - 2025-01-02

- **Breaking Changes**: 없음
- **Migration**: 불필요
- **Commit**: 210e02e

### 1. Fail-Open Strategy for Configuration Errors

플러그인 설정 오류 시 Pod 장애를 방지하는 fail-open 전략을 구현.

**주요 변경사항**:

1. **설정 오류 추적 시스템**
   - `config_error` 필드 추가 (RootContext, HttpContext)
   - 명확한 에러 코드 체계:
     - `ERR_NO_CONFIG`: 설정 없음
     - `ERR_UTF8`: UTF-8 인코딩 오류
     - `ERR_JSON`: JSON 파싱 실패
     - `ERR_PARSE`: 설정 로직 오류

2. **Pod 안전성 확보**
   - `on_configure()` 실패 시 `false` 대신 `true` 반환
   - 설정 오류가 있어도 Pod 정상 시작
   - 트래픽 중단 방지

3. **Graceful Degradation**
   - 설정 오류 시 필터 자동 우회 (bypass)
   - 트래픽은 정상 처리, 메트릭 수집만 중단
   - `on_http_request_headers()`에서 config_error 체크

4. **운영 모니터링 개선**
   - 설정 실패 시 error 로그 (1회만):
     ```
     [oef] ❌ (ERR_PARSE) Configuration failed: ...
     [oef] ⚠️  All requests will bypass filter (no metrics collected)
     ```
   - 요청 처리 시 debug 로그 (로그 폭주 방지)
   - 메트릭 헤더 자동 설정:
     - `x-api-endpoint: "config-error"`
     - `x-service-name: "config-error"`
     - `x-path-template: "config-error"`
   - Prometheus/Grafana에서 설정 오류 모니터링 가능

**영향**:
- ✅ 설정 오류로 인한 Pod 시작 실패 방지
- ✅ 잘못된 설정 배포 시에도 트래픽 정상 처리
- ✅ 로그 폭주 없이 운영 문제 인지 가능
- ✅ 메트릭 기반 알람 설정 가능

### 2. Atomic Configuration Update

설정 업데이트 실패 시 부분 적용을 방지하는 원자적 설정 업데이트를 구현.

**주요 변경사항**:

1. **4단계 원자적 업데이트**
   - Phase 1: 모든 값 먼저 파싱 및 검증 (self 수정 없음)
   - Phase 2: 새 라우터 완전히 구축 (실패해도 self 안전)
   - Phase 3: 새 캐시 구축
   - Phase 4: 모든 검증 통과 후 원자적으로 적용

2. **트랜잭션 보장**
   - 설정 파싱/검증 중 에러 발생 시 `self` 전혀 변경 안 됨
   - All-or-nothing: 모든 검증 통과 또는 완전 롤백
   - 부분 적용으로 인한 불일치 상태 완전 차단

**영향**:
- ✅ 설정 실패 시 이전 상태 완전 보존
- ✅ Pod 일관성 보장
- ✅ 부분 적용 문제 완전 해결

### 3. Enhanced Configuration Validation

런타임 에러를 방지하기 위한 포괄적인 설정 검증을 구현.

**추가된 검증**:
- ✅ 빈 services 배열
- ✅ 빈 서비스 이름
- ✅ 빈 paths 객체
- ✅ '/'로 시작하지 않는 경로
- ✅ 1024자 초과 경로
- ✅ Null 문자 포함 경로
- ✅ 공백 포함 경로
- ✅ 개행 문자 포함 경로
- ✅ 중복 경로 (명확한 에러 메시지)

**영향**:
- ✅ 설정 오류 시 명확한 에러 메시지로 디버깅 용이
- ✅ 런타임 크래시 방지
- ✅ 잘못된 설정 조기 발견

### 4. Enhanced Path Normalization

논리적으로 동일한 경로가 형식 차이로 매칭 실패하는 문제를 해결하기 위해 포괄적인 경로 정규화를 구현.

**주요 변경사항**:

1. **normalize_path() 함수 추가**
   - Query string 제거: `/users?key=value` → `/users`
   - Fragment 제거: `/users#section` → `/users`
   - Trailing slash 제거: `/users/` → `/users` (루트 `/`는 유지)
   - 중복 슬래시 제거: `/users//profile` → `/users/profile`

2. **테스트 커버리지 확장**
   - 15개의 정규화 테스트 케이스 추가
   - 실제 매칭 시나리오 검증

**영향**:
- ✅ 캐시 히트율 향상
- ✅ 매칭 정확도 개선
- ✅ 엣지 케이스 처리 강화

## [0.3.0] - Previous Release

**Commit**: c0a955b

### Added
- `preserveExistingHeaders` 설정 추가

---

## 알려진 이슈

다음 이슈들은 별도로 수정 예정입니다 (상세 내용: `issue.md` 참조):

### 🔴 치명적 이슈
1. **HTTP Method 미사용** (`src/lib.rs:280-320`)
   - `GET /users`와 `POST /users`를 같은 엔드포인트로 처리
   - 라우팅 키에 method 추가 필요

2. **Hostname 미사용** (`src/lib.rs:173`)
   - Virtual hosting 불가능
   - 라우팅 키에 hostname 추가 필요

3. **캐시 키 불완전** (`src/lib.rs:211-220`)
   - 캐시 키에 method, hostname 누락
   - 잘못된 캐시 히트 가능

### 🟡 중간 수준 이슈
- 설정 핫 리로드 시 캐시 불일치
- 모호한 "unknown" 처리

자세한 내용은 `issue.md`를 참조하세요.
