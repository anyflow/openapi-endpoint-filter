FROM scratch
COPY target/wasm32-unknown-unknown/release/openapi_path_filter.optimized.wasm /plugin.wasm
