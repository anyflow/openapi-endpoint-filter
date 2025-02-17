FROM scratch
COPY target/wasm32-unknown-unknown/release/openapi_path_filter.wasm /plugin.wasm
