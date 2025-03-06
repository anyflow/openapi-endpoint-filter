# openapi-path-filter

## Introduction

A Rust-based Istio Proxy-Wasm filter that injects the OpenAPI based path template of a request path into the request header. The header value can be used in a Istio metric label.

## Features

- **OpenAPI path template 식별**: request path에 해당하는 path template을 `x-openapi-path` request header로 추가
  - **`x-openapi-path` 값을 Istio metric label로 추가 가능**: Istio `Telemetry` API의 `tagOverrides` 사용을 통해. [`./resources/telemetry.yaml`](./resources/telemetry.yaml) 참조
- **고성능 path matching**: Radix tree 기반의 `matchit` crate 사용. [benchmark 결과 가장 빠르다고](https://github.com/ibraheemdev/matchit?tab=readme-ov-file#benchmarks).
- **OpenAPI path syntax 사용**: OpenAPI 문서 변환 불필요. 문서 그대로 config에 삽입 가능.(가독성을 위해 path template 이외의 항목 제거 추천). [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml) 참조
- **다수의 OpenAPI 문서 지원**: `name` field는 해당 OpenAPI 문서의 서비스 명칭으로 `x-service-name` request header로 추가. [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml) 참조

- **LRU cache 지원**: config로 cache size 조절 가능. [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml) 참조

## Getting started

```shell
# Rust 설치. 참고로 macOS에서 brew로 설치하면 정상 compile안됨. 따라서 Rust 공식 설치 Path를 따라야.
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# cargo-make 설치 (빌드 도구. Makefile.toml 참고)
> cargo install cargo-make

# wasm-opt (bynaryen) 설치 (macOS의 경우. 타 OS의 경우 별도 방법 필요. 설치 안될 경우 Makefile.toml의 optimize-wasm task 제거로 본 step skip 가능)
> brew install binaryen

# .env을 root에 생성 및 DOCKER_IMAGE_PATH 설정. 아래는 예
DOCKER_IMAGE_PATH=anyflow/openapi-path-filter

# test -> rust build -> image optimization -> docker build -> docker push
> cargo make deploy
```

## runtime 테스트 방법 in Istio

```shell

# 대상 pod wasm log level을 debug로 변경
> istioctl pc log -n <namespace name> <pod name> --level wasm:debug

# openapi-path-filter 만 logging
> k logs -n <namespace name> <pod name> -f | grep -F '[opf]'

# resource/telemetry.yaml 적용: x-openapi-path header, method를 각각 request_path, request_method란 metric label로 넣기 위함
> kubectl apply -f telemetry.yaml

# resources/wasmplugin.yaml 적용: 정상 loading 여부 확인을 위한 log 확인. e.g. "[opf] Router configured successfully"
> kubectl apply -f wasmplugin.yaml

# curl로 호출 후 log에 matching 여부 matching 성공 log가 나오는지 확인.
# e.g. [opf] /dockebi/v1/stuff matched and cached with dockebi, /dockebi/v1/stuff
```

## License

openapi-path-filter is released under version 2.0 of the Apache License.

## 참고

### `[opf]` log prefix에 관하여

log grep 용. `openapi-path-filter` 만 갖고는 전체 `grep` 불가하기 때문. 아래 첫 번째처럼 `openapi-path-filter` 가 자동으로 붙는 경우도 있지만 두 번째처럼 안붙는 경우도 있기 때문.

- `2025-02-23T20:30:59.970615Z     debug   envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1192 wasm log cluster.openapi-path-filter: [opf] Creating HTTP context       thread=29`
- `2025-02-23T20:28:39.632084Z     info    envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1195 wasm log: [opf] Router configured successfully  thread=20`


### wasm unloading 확인 방법에 관하여

`kubectl delete -f wasmplugin.yaml` 을 하더라도 그 즉시 wasm이 Envoy에서 삭제되는 것이 아닌 30s ~ 60s이 지난 후에 삭제되는 듯. 아래와 같은 로그로 확인 가능. 새로운 wasm 동작 확인 필요 시 기존 wasm 제거 후 아래 메시지 확인 후 새 wasm 로드 필요.

- `2025-02-23T19:35:58.014282Z     info    envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1195 wasm log: openapi-path-filter terminated        thread=20`
