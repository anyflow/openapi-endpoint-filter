# Elikas RUST

## TODO

- ✅ 정상 등록 및 실제 동작 검증
- ✅ 동적 wasm 모듈 로딩 테스트
- 🚧 동적 `WasmPlugin` 로딩 테스트: configuration 동적 업데이트
- [FIX] log가 안찍힘
- [최적화] Rust 언어 관점, biz logic 관점

## Getting started

```shell
# Rust 설치. 참고로 macOS에서 brew로 설치하면 정상 compile안됨. 따라서 Rust 공식 설치 Path를 따라야.
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# target 설치
> rustup target add wasm32-unknown-unknown

# build
> cargo build --target wasm32-unknown-unknown --release

# docker build
> docker build -t docker-registry.anyflow.net/openapi-path-filter:latest .

# docker push
> docker push docker-registry.anyflow.net/openapi-path-filter:latest

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
