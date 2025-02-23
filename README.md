# openapi-path-filter

## Introduction

A Rust-based Istio WASM filter that injects a Prometheus label representing the request path, based on a path defined in the OpenAPI spec.

## Advantages

- URL matching 최적화: `matchit` lib 사용(benchmark 결과 가장 빠르다고. [참조](https://github.com/ibraheemdev/matchit?tab=readme-ov-file#benchmarks))

## DONE

- ✅ 정상 등록 및 실제 동작 검증
- ✅ 동적 wasm 모듈 로딩 테스트
- ✅ 단위테스트 보강
- ✅ build 자동화: cargo-make 사용
- ✅ cargo/docker image version 자동 동기화(`${CARGO_MAKE_CRATE_VERSION}` in `Makefile.toml`)
- ✅ image optimization (`wasm-opt` 도입)
- ✅ Fast fail, optimization 포함 build step 정렬
- 💧 LRU 캐시 도입: lru cache 사용이 적절하지만 read에 조차 lock을 써야하기에 오히려 성능 저하 크고 복잡도가 증가. `cache` branch 참조.

## TODO

- `proxy-wasm-test-framework = { git = "https://github.com/proxy-wasm/test-framework" }` 사용하여 테스트 가능하도록: `hostcall`에서 오는 call path 검증용

## 참고

`kubectl delete -f wasmplugin.yaml` 을 하더라도 그 즉시 wasm이 Envoy에서 삭제되는 것이 아닌 약간(30초?) 시간이 지난 후에 삭제되는 듯. 아래와 같은 로그로 확인 가능. 새로운 wasm 동작 확인 필요 시 기존 wasm 제거 후 아래 메시지 확인 후 새 wasm 로드 필요.

`2025-02-23T05:51:26.936732Z     debug   envoy init external/envoy/source/common/init/target_impl.cc:68  shared target FilterConfigSubscription init extenstions.istio.io/wasmplugin/cluster.openapi-path-filter destroyedthread=20`

## Getting started

```shell
# Rust 설치. 참고로 macOS에서 brew로 설치하면 정상 compile안됨. 따라서 Rust 공식 설치 Path를 따라야.
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# cargo-make 설치 (빌드 도구. Makefile.toml 참고)
> cargo install cargo-make

# wasm-opt (bynaryen) 설치 (macOS의 경우. 타 OS의 경우 별도 방법 필요. 설치 안될 경우 Makefile.toml의 optimize-wasm task 제거로 본 step skip 가능
> brew install binaryen

# test -> rust build -> image optimization -> docker build -> docker push
> cargo make clean-all

# 정상 등록 여부 확인
> curl -X GET https://docker-registry.anyflow.net/v2/openapi-path-filter/manifests/latest \
  -H "Accept: application/vnd.oci.image.manifest.v1+json"

  # output
  {
    "schemaVersion": 1,
    "name": "openapi-path-filter",
    "tag": "latest",
    "architecture": "arm64",
    "fsLayers": [
        {
          "blobSum": "sha256:320faed3ae036840fc0b77a5ca2090383970865d3da8963e196a47b777de940a"
        }
    ],
    "history": [
        {
          "v1Compatibility": "{\"architecture\":\"arm64\",\"config\":{\"Env\":[\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"],\"WorkingDir\":\"/\"},\"created\":\"2025-02-17T14:59:27.382128345Z\",\"id\":\"dc62aa1b573b31b49429debbedaab6291dcdee8a1629919ca506f3e9f6445202\",\"os\":\"linux\"}"
        }
    ],
    "signatures": [
        {
          "header": {
              "jwk": {
                "crv": "P-256",
                "kid": "3NDH:3O7V:WTTD:BNBT:HLPN:TRW2:JUHL:AARV:K6XM:OVTZ:S2ZO:7NNI",
                "kty": "EC",
                "x": "tGUu0g9-c7mmx0-0I1C32uR6agE_TXr9Ar7DiesUTz0",
                "y": "l7HXWD-VRz6As495iB0t4GOyY-XgqcKBXguVqJvm_0w"
              },
              "alg": "ES256"
          },
          "signature": "H35A8aksiOzN9D_8FQGkSILPiUS5VjkgzvuDSqog4eu6WSFYUpTgwm58JES1KUiV1X279oZu-BBk6-1dGBqwuw",
          "protected": "eyJmb3JtYXRMZW5ndGgiOjU4OSwiZm9ybWF0VGFpbCI6IkNuMCIsInRpbWUiOiIyMDI1LTAyLTE3VDE2OjA4OjQzWiJ9"
        }
    ]
  }
```

## docker-registry 참고 명령어

```shell
# image catalog 얻기
❯ curl -X GET https://docker-registry.anyflow.net/v2/_catalog
{"repositories":["api-signature-filter","doc-publisher","dockebi","docserver","manifest-generator","openapi-path-filter","staffonly"]}


# tag 목록 얻기
❯ curl -X GET https://docker-registry.anyflow.net/v2/openapi-path-filter/tags/list
{"name":"openapi-path-filter","tags":["1.0.0","latest","0.1.0"]}

# image digest 얻기
❯ curl -X GET https://docker-registry.anyflow.net/v2/openapi-path-filter/manifests/latest \
  -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
  -v 2>&1 | grep docker-content-digest | awk '{print ($3)}'
sha256:956f9ebd2cd44b60e82d5cfc0e2b9c12ca04e61532d8e024a4cc712dea011277

# image 삭제하기 (REGISTRY_STORAGE_DELETE_ENABLED=true 설정 필요 in docker-registry)
curl -X DELETE https://docker-registry.anyflow.net/v2/openapi-path-filter/manifests/<image digest>
```
