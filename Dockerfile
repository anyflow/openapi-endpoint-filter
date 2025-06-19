FROM scratch
COPY target/wasm32-unknown-unknown/release/openapi_endpoint_filter.optimized.wasm /plugin.wasm
