FROM scratch
COPY target/wasm32-unknown-unknown/release/path_template_filter.optimized.wasm /plugin.wasm
