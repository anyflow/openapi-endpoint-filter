# Tasks

- ✅ 정상 등록 및 실제 동작 검증
- ✅ 동적 wasm 모듈 로딩 테스트
- ✅ 단위 테스트 보강
- ✅ build 자동화: `cargo-make` 사용
- ✅ cargo/docker image version 자동 동기화(`${CARGO_MAKE_CRATE_VERSION}` in `Makefile.toml`)
- ✅ image optimization (`wasm-opt` 도입)
- ✅ Fast fail, optimization 포함 build step 정렬
- ✅ Single thread 용으로 전환(`Rc<T>` 사용). proxy WASM은 single thread로 동작하므로
- ✅ **LRU 캐시 도입**: `lru` lib 사용. single thread 환경이므로 lock 고민 불필요
	- ✅ **cache_size < 0 or invalid(없음 포함)**: default 처리 (1024)
	- ✅ **cache_size == 0**: cache disable
- ✅ Service 별 openapi 삽입 가능하도록. service name은 `x-service-name` header로 전달
- 🚧 [`EnvoyFilter`](https://istio.io/v1.11/docs/ops/configuration/extensibility/wasm-module-distribution/) 를 이용한 WASM loading for vm ID 일치화
- 🚧 예외 처리: image 없을 경우 host에 outage 발생 안하도록
- 🚧 `proxy-wasm-test-framework = { git = "https://github.com/proxy-wasm/test-framework" }` 사용하여 테스트 가능하도록: runtime 검증용. 이게 되기 전까지는 [runtime 테스트 방법 in istio](#runtime-테스트-방법-in-istio) 로 검증해야.