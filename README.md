# `path-template-filter`

## Introduction

A Rust-based Istio/Envoy Proxy-Wasm plugin that injects the OpenAPI-derived path template of a request path into the request header, enabling its use as a value in Istio metric labels.

## Motivation

- The smallest unit for traffic identification in Istio metrics is the workload. However, Identification at the endpoint (path + method) level is highly useful in the real world.
- While the Istio `Telemetry` API allows adding method and path labels for identification, it cannot map a path to its corresponding template that includes any path parameters, making endpoint-level identification impossible.
- [Classifying Metrics Based on Request or Response](https://istio.io/latest/docs/tasks/observability/metrics/classify-metrics/) discusses endpoint-level identification using the [`attributegen`](https://github.com/istio-ecosystem/wasm-extensions/tree/master/extensions/attributegen) plugin, but the plugin's path matching algorithm relies on regex and scan operations, resulting in performance issues. Additionally, converting OpenAPI path templates into regex-based patterns is required.

## Features

- **OpenAPI Path Template Identification**: Adds the path template corresponding to the request path as the `x-path-template` request header.
  - **Add `x-path-template` value as an Istio metric label**: Achievable using the `tagOverrides` in the Istio `Telemetry` API. Refer to [`./resources/telemetry.yaml`](./resources/telemetry.yaml).
- **High-Performance Path Matching**: Utilizes the Radix tree-based path matching algorithm via `matchit` crate, [whose own benchmarks report it as the fastest](https://github.com/ibraheemdev/matchit?tab=readme-ov-file#benchmarks).
- **Uses OpenAPI Path Syntax**: No need to transform OpenAPI documents. You can insert them directly into the config as-is (though removing items other than path templates is recommended for readability). Refer to [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml).
- **Support for Multiple OpenAPI Documents**: The `name` field represents the service name of the respective OpenAPI document and is added as the `x-service-name` request header, which can be used in the same manner as `x-path-template` for Istio metric label. Refer to [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml).

- **LRU Cache Support**: Cache size can be adjusted via config. Refer to [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml).

## Getting started

```shell
# Install Rust. Note: Installing via brew on macOS may not compile correctly. Follow the official Rust installation path instead.
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install cargo-make (build tool; refer to Makefile.toml).
> cargo install cargo-make

# Install wasm-opt (binaryen) (for macOS; other OSes require a different method. If installation fails, skip this step by removing the optimize-wasm task from Makefile.toml).
> brew install binaryen

# Create a .env file at the root and set DOCKER_IMAGE_PATH. Example below:
DOCKER_IMAGE_PATH=anyflow/path-template-filter

# Run tests -> Rust build -> Image optimization -> Docker build -> Docker push
> cargo make deploy
```

## How to Test at Runtime in Istio

```shell
# Change the WASM log level of the target pod to debug.
> istioctl pc log -n <namespace name> <pod name> --level wasm:debug

# Filter logs to show only path-template-filter.
> k logs -n <namespace name> <pod name> -f | grep -F '[ptf]'

# Apply resources/telemetry.yaml: To use the x-path-template, x-service-name headers and the method as metric labels `request_path`, `request_service` and `request_method`.
> kubectl apply -f telemetry.yaml

# Apply resources/wasmplugin.yaml: Check logs to confirm successful loading, e.g., "[ptf] Router configured successfully".
> kubectl apply -f wasmplugin.yaml

# Make a curl request and verify if the matching success log appears, e.g., "[ptf] /dockebi/v1/stuff matched and cached with dockebi, /dockebi/v1/stuff".
```

## Notes

### About the `[ptf]` Log Prefix

Used for log grepping. Grepping with just path-template-filter` isn’t feasible because, as shown below, it’s automatically included in some cases but not in others.

- `2025-02-23T20:30:59.970615Z debug envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1192 wasm log cluster.path-template-filter: [ptf] Creating HTTP context thread=29`
- `2025-02-23T20:28:39.632084Z info envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1195 wasm log: [ptf] Router configured successfully thread=20`

### About Verifying WASM Unloading

Even after running `kubectl delete -f wasmplugin.yaml`, the WASM isn’t immediately removed from Envoy; it seems to take 30–60 seconds. You can confirm this with logs like the one below. If you need to test new WASM behavior, remove the existing WASM, wait for the message below, and then load the new WASM.

- `2025-02-23T19:35:58.014282Z     info    envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1195 wasm log: path-template-filter terminated        thread=20`

## License

`path-template-filter` is released under version 2.0 of the Apache License.
