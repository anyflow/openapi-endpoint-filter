# `openapi-endpoint-filter` for Istio/Envoy

A Rust-based Proxy-Wasm filter that injects **OpenAPI operations** (e.g., `GET /users/{id}`) into request headers, which can be mapped as **Istio metric labels** via the `Telemetry` API to enable **endpoint-level observability without cardinality explosion**.

## 💡 Features

- **OpenAPI Endpoint Identification**: Adds the endpoint corresponding to the request path as the `x-api-endpoint` request header (e.g., `GET /foo/{foo-id}`), as well as the `x-path-template` header (e.g., `/foo/{foo-id}`) for the path template.
  - **Map headers to Istio metric labels**: Use `tagOverrides` in the Istio `Telemetry` API; the WASM only creates headers and does not alter metrics directly. Refer to [`./resources/telemetry.yaml`](./resources/telemetry.yaml).
- **High-Performance Path Matching**: Utilizes the Radix tree-based path matching algorithm via the `matchit` crate, [whose own benchmarks report it as the fastest](https://github.com/ibraheemdev/matchit?tab=readme-ov-file#benchmarks).
- **Uses OpenAPI Path Syntax**: There is no need to transform OpenAPI documents. You can insert them directly into the config as-is (although removing items other than path templates is recommended for readability). Refer to [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml).
- **Support for Multiple OpenAPI Documents**: The `name` field represents the service name of the respective OpenAPI document and is added as the `x-service-name` request header, which can be used in the same manner as `x-api-endpoint` for Istio metric labels. Refer to [`./resources/wasmplugin.yaml`](./resources/wasmplugin.yaml).

## 🧩 How It Works

```text
Request → Istio Proxy (Sidecar or Ingress Gateway or Waypoint)
          → openapi-endpoint-filter (Proxy-Wasm)
            → x-api-endpoint header injection
            → request_endpoint label mapping (via Istio Telemetry API)
```

## 🚀 Quickstart (5 minutes)

### Prerequisites

- OpenAPI 3.x specification file

### 1. Configure & Apply the Istio `WasmPlugin` manifest

Create an Istio `WasmPlugin` manifest by referring to [`resources/wasmplugin.yaml`](./resources/wasmplugin.yaml) and apply it.

### 2. Configure & Apply the Istio `Telemetry` manifest (Optional for labeling Istio metrics)

Create or update an Istio `Telemetry` manifest by referring to [`resources/telemetry.yaml`](./resources/telemetry.yaml) and apply it.

### 3. Verify

Run a `curl` command to test the endpoint:

```shell
curl <SERVICE_URL>/users/123
```

Verify that the injected headers are present:

```shell
# Expected output:
x-api-endpoint: GET /users/{id}
x-path-template: /users/{id}
x-service-name: userservice
```

Then, verify the metrics (e.g., in Prometheus):

```shell
# Query:
sum by (request_service, request_endpoint, request_path_template) (istio_requests_total)

# Expected output:
request_service="userservice", request_endpoint="GET /users/{id}", request_path_template="/users/{id}"
```

## Motivation

- The smallest unit for traffic identification in Istio metrics is the workload. However, identification at the endpoint (path + method) level is highly useful in real-world scenarios.
- While the Istio `Telemetry` API allows adding method and path labels for identification, it cannot map a path to its corresponding template that includes path parameters, making endpoint-level identification impossible.
- [Classifying Metrics Based on Request or Response](https://istio.io/latest/docs/tasks/observability/metrics/classify-metrics/) discusses endpoint-level identification using the [`attributegen`](https://github.com/istio-ecosystem/wasm-extensions/tree/master/extensions/attributegen) plugin, but the plugin's path matching algorithm relies on regex and scan operations, resulting in performance issues. Additionally, converting OpenAPI path templates into regex-based patterns is required.

## Behavior Notes

- **Matching key**: Requests are matched using host (and basePath from OpenAPI `servers`), HTTP method, and normalized path template.
- **Host matching toggle**: If `useHostInMatch` is `false`, host is ignored and only basePath/method/path are used (basePath matching still applies).
- **Header preservation**: `preserveExistingHeaders` default: `true`. When enabled, if the request already includes `x-api-endpoint`, `x-path-template`, or `x-service-name`, the WASM does not recompute or replace them.
- **Matching fallback**: If no route matches, the plugin sets `unknown` values (e.g., `x-api-endpoint: <METHOD> unknown`, `x-path-template: unknown`, `x-service-name: unknown`).
- **Config errors**: On config parse errors, the filter fails open and injects `config-error` into all three headers for observability.
- **Host/method rules**:
  - Host is read from `:authority` or `host`, lowercased, and port-stripped.
  - If a path item has no HTTP methods, all methods are allowed for that path.
- **OpenAPI servers**: `servers.url` and `variables` are expanded for host/basePath matching (max 100 expansions).

## Configuration for Istio

- **`wasmplugin.yaml`**: Register OpenAPI path templates and service names. You can specify multiple services and their paths at once.
  - `useHostInMatch`: Whether to match request host against servers.url host (default: `true`)
  - `preserveExistingHeaders`: Preserve existing `x-*` headers from upstream (default: `true`)
  - `services`: List of service names and their OpenAPI path templates
- **`telemetry.yaml`**: Maps the headers added by the plugin (`x-api-endpoint`, `x-path-template`, `x-service-name`) to Istio metric labels using `tagOverrides`. The `tagOverrides` keys are the metric label names (e.g., `request_endpoint`, `request_path_template`, `request_service`) and the values read from request headers.

## Examples

### Header injection

Minimal example showing how a request maps to injected headers and metric labels (when Telemetry API tagOverrides is enabled).

```yaml
# Config (partial)
pluginConfig:
  services:
    - name: userservice
      servers:
        - url: https://api.example.com/v1
      paths:
        /users/{id}: {}

# Request
:method: GET
:authority: api.example.com
:path: /v1/users/42

# Output headers
x-api-endpoint: GET /users/{id}
x-path-template: /users/{id}
x-service-name: userservice

# Output labels on Istio metrics (e.g., istio_requests_total) via Telemetry tagOverrides.
istio_requests_total{
  request_endpoint="GET /users/{id}",
  request_path_template="/users/{id}",
  request_service="userservice",
  request_method="GET", # Telemetry API (not from openapi-endpoint-filter)
  request_host="api.example.com" # Telemetry API (not from openapi-endpoint-filter)
}
```

### Server variables

Shows host matching across multiple server variable expansions.

```yaml
# Config (partial)
pluginConfig:
  services:
    - name: userservice
      servers:
        - url: https://{env}.example.com/v1
          variables:
            env:
              default: api
              enum: [api, staging]
      paths:
        /users/{id}: {}

# Requests
- method: GET
  host: api.example.com
  path: /v1/users/42
- method: GET
  host: staging.example.com
  path: /v1/users/42

# Output headers
x-api-endpoint: GET /users/{id}
x-path-template: /users/{id}
x-service-name: userservice

# Output labels on Istio metrics (e.g., istio_requests_total) via Telemetry tagOverrides.
istio_requests_total{
  request_endpoint="GET /users/{id}",
  request_path_template="/users/{id}",
  request_service="userservice",
  request_method="GET", # Telemetry API (not from openapi-endpoint-filter)
  request_host="<api.example.com | staging.example.com>" # per request host; Telemetry API (not from openapi-endpoint-filter)
}
```

For detailed examples, refer to each config file in [resources](./resources/) directory

## Use Prebuilt Image

You can use the published image `anyflow/openapi-endpoint-filter:<version>` without building locally. Refer to [`resources/wasmplugin.yaml`](./resources/wasmplugin.yaml) for configuration.

## Build & Publish

```shell
# Install Rust. Note: Installing via brew on macOS may not compile correctly. Follow the official Rust installation path instead.
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install cargo-make (build tool; refer to Makefile.toml).
> cargo install cargo-make

# Install wasm-opt (binaryen) (for macOS; other OSes require a different method. If installation fails, you can skip this step by removing the optimize-wasm task from Makefile.toml).
> brew install binaryen

# Create a .env file at the root and set DOCKER_IMAGE_PATH. Example below:
DOCKER_IMAGE_PATH=anyflow/openapi-endpoint-filter

# Run tests -> Rust build -> Image optimization -> Docker build -> Docker push
> cargo make deploy
```

## How to Test at Runtime in Istio

```shell
# Change the WASM log level of the target pod to debug.
> istioctl pc log -n <namespace name> <pod name> --level wasm:debug

# Filter logs to show only openapi-endpoint-filter.
> k logs -n <namespace name> <pod name> -f | grep -F '[oef]'

# Apply resources/telemetry.yaml: To use the x-path-template, x-service-name headers and the method as metric labels `request_path`, `request_service` and `request_method`.
> kubectl apply -f telemetry.yaml

# Apply resources/wasmplugin.yaml: Check the logs to confirm successful loading, e.g., "[oef] Router configured successfully".
> kubectl apply -f wasmplugin.yaml

# Make a curl request and verify if the matching success log appears, e.g., "[oef] /dockebi/v1/stuff matched with dockebi, /dockebi/v1/stuff".
```

## Notes

### About the `[oef]` Log Prefix

This is used for log grepping. Grepping with just `openapi-endpoint-filter` isn't feasible because, as shown below, it's automatically included in some cases but not in others.

- `2025-02-23T20:30:59.970615Z debug envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1192 wasm log cluster.openapi-endpoint-filter: [oef] Creating HTTP context thread=29`
- `2025-02-23T20:28:39.632084Z info envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1195 wasm log: [oef] Router configured successfully thread=20`

### About Verifying WASM Unloading

Even after running `kubectl delete -f wasmplugin.yaml`, the WASM isn't immediately removed from Envoy; it seems to take 30–60 seconds. You can confirm this with logs like the one below. If you need to test new WASM behavior, remove the existing WASM, wait for the message below, and then load the new WASM.

- `2025-02-23T19:35:58.014282Z     info    envoy wasm external/envoy/source/extensions/common/wasm/context.cc:1195 wasm log: openapi-endpoint-filter terminated        thread=20`

## License

`openapi-endpoint-filter` is released under the Apache License, version 2.0.
