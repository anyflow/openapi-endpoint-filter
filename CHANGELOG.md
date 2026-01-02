# Changelog

## [0.4.0](https://github.com/anyflow/openapi-endpoint-filter/commit/a6512ec) (2026-01-03)

### Features
- OpenAPI 3.0 `servers`/`variables` 기반 host/basePath 매칭 지원
- method/host 기반 라우팅 지원 (method 미지정 시 전체 허용)
- `useHostInMatch` 옵션으로 요청 host 매칭 선택 지원

### Miscellaneous Chores
- 라우팅/설정 파싱 로직 모듈 분리
- scheme 매칭 제거 및 관련 로직 정리
- 테스트 경고 정리 및 문서/설정 업데이트

### Notes
- 설정 스키마 하위 호환(기존 설정 그대로 사용 가능)

## [0.3.2](https://github.com/anyflow/openapi-endpoint-filter/commit/26c71e3) (2026-01-02)

### Miscellaneous Chores
- ⚠️ 캐시 기능 제거 (`cacheSize` 설정 무시)
- LRU 캐시 제거 및 관련 로그/테스트 단순화
- `lru` 의존성 제거

## [0.3.1](https://github.com/anyflow/openapi-endpoint-filter/commit/210e02e) (2025-01-02)

### Features
- 설정 오류 시 fail-open 동작 및 `config-error` 메트릭 헤더 추가
- 원자적 설정 업데이트 (부분 적용 방지)
- 강화된 설정 검증 및 경로 정규화

### Miscellaneous Chores
- Single thread 환경에 맞춰 `Rc<T>` 사용
- 단위 테스트 보강

## [0.3.0](https://github.com/anyflow/openapi-endpoint-filter/commit/c0a955b) (Previous Release)

### Features
- `preserveExistingHeaders` 설정 추가
- Service 별 OpenAPI 삽입 및 `x-service-name` 헤더 지원
- LRU 캐시 도입 (`cacheSize` 기본값/disable 동작 포함)

### Miscellaneous Chores
- 정상 등록 및 실제 동작 검증
- 동적 WASM 모듈 로딩 테스트
- build 자동화 (`cargo-make` 사용)
- cargo/docker 이미지 버전 자동 동기화
- image optimization (`wasm-opt` 도입)
- Fast fail 포함 build step 정렬
- WASM logic 오류 시 WASM bypass하도록: `failStrategy: FAIL_OPEN` 을 `WasmPlugin` 에 적용

### Notes
- `FAIL_CLOSE` 시 5XX error 발생(`RootContext`에서는 panic시 즉각 500, `HttpContext`에서는 장시간 no response 및 강제 connection 종료 시 503 발생. 테스트 시 기존 WASM이 terminated된 상태를 반드시 확인하고 WASM을 load해야 기존 configuration로 인한 오동작을 피할 수 있음)
- WASM image이 없는 경우에는 `failStrategy` 에 관계 없이 bypass로 동작함

## TODO
- EnvoyFilter 를 이용한 WASM loading for vm ID 일치화: metric을 통해 기본 동작이 어떻게 되는지를 좀더 살필 필요 있음. Default로 WASM VM 또는 runtime이 동작하여 이를 활용한다는 이야기도 있고
- proxy-wasm-test-framework = { git = "https://github.com/proxy-wasm/test-framework" } 사용하여 테스트 가능하도록: runtime 검증 용이성 증대. 이게 되기 전까지는 runtime 테스트 방법 in istio 로 검증해야
