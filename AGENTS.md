# Repository Guidelines

## Project Summary
- This repository is a single-crate Rust `cdylib` that builds a Proxy-Wasm filter for Istio/Envoy.
- The filter parses OpenAPI-like configuration, builds host/basePath/method/path routers, and injects `x-api-endpoint`, `x-path-template`, and `x-service-name` request headers.
- The current implementation intentionally fails open on configuration errors and emits `config-error` headers for observability instead of blocking requests.

## Repository Layout
- `src/lib.rs`: root context, HTTP context, configuration loading, header injection, and the inline unit test suite.
- `src/config.rs`: parsing and validation of OpenAPI service definitions, server expansion, method extraction, and route insertion helpers.
- `src/router.rs`: path normalization and route matching grouped by host and base path.
- `resources/wasmplugin.yaml`: example Istio `WasmPlugin` manifest.
- `resources/telemetry.yaml`: example Istio `Telemetry` manifest for mapping headers to metric labels.
- `README.md`: user-facing behavior, install flow, and operational guidance.
- `Makefile.toml`: `cargo-make` tasks for test/build/optimize/docker/push workflows.

## Working Rules
- Keep the code dependency-light. Do not add new crates unless the user explicitly asks for them.
- Prefer extending the existing `config` and `router` modules over adding new layers or abstractions.
- Preserve the fail-open behavior unless the user explicitly asks to change request-handling semantics.
- Preserve path normalization behavior unless tests and README examples are updated together.
- Keep logs compatible with the existing `[oef]` prefix because the README documents log-grep workflows around it.

## Change Guidance
- If routing behavior changes, update or add unit tests in `src/lib.rs` first.
- If plugin configuration keys or defaults change, update all three:
  - `README.md`
  - `resources/wasmplugin.yaml`
  - relevant parsing logic in `src/lib.rs` / `src/config.rs`
- If output headers or their meaning change, update both:
  - `README.md`
  - `resources/telemetry.yaml`
- If the release version changes, keep at least these files in sync:
  - `Cargo.toml`
  - `resources/wasmplugin.yaml`
  - any README examples that mention the published image tag

## Verification
- Default verification for code changes: `cargo test`
- Formatting: `cargo fmt`
- Preferred static check when logic changes meaningfully: `cargo clippy --all-targets --all-features`
- WASM build verification when build pipeline changes: `cargo build --target wasm32-unknown-unknown --release`
- Full release pipeline, only when explicitly needed and environment is prepared: `cargo make deploy`

## Environment Notes
- `cargo make deploy` expects `.env` with `DOCKER_IMAGE_PATH`.
- `cargo make deploy` also expects `cargo-make`, Docker, and `wasm-opt` to be available.
- Runtime validation described in `README.md` assumes an Istio environment and is not locally reproducible from this repository alone.

## Testing Notes
- The test suite currently lives in `src/lib.rs` under `#[cfg(test)]`.
- Existing tests cover path normalization, server expansion, host matching, method matching, and invalid configuration handling.
- When fixing edge cases, prefer adding focused unit coverage close to the current inline tests rather than creating a new test harness.
