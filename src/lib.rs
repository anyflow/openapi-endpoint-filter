use std::sync::Arc;
use proxy_wasm::{
    traits::{Context, HttpContext, RootContext},
    types::*,
};
use serde_json::Value;
use matchit::Router;
use log::{info};
// use log::{info, warn, error};

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> {
        Box::new(OpenapiPathRootContext {
            router: Arc::new(Router::new()), // Arc 사용
        })
    });
}}

struct OpenapiPathHttpContext {
    router: Arc<Router<String>>, // Arc로 공유
}

impl Context for OpenapiPathHttpContext {}

impl HttpContext for OpenapiPathHttpContext {
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action {
        if let Some(path) = self.get_http_request_header(":path") {
            match self.router.at(&path) {
                Ok(matched) => {
                    let openapi_path = matched.value;
                    self.set_http_request_header("x-openapi-path", Some(&openapi_path));
                }
                Err(_) => {
                    // Handle unmatched routes (optional)
                }
            }
        }
        Action::Continue
    }
}

struct OpenapiPathRootContext {
    router: Arc<Router<String>>, // Arc 사용
}

impl Context for OpenapiPathRootContext {}

impl RootContext for OpenapiPathRootContext {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool {
        info!("openapi-path-filter successfully created and started!");
        true
    }

    fn on_configure(&mut self, _: usize) -> bool {
        // Get plugin configuration
        if let Some(config_bytes) = self.get_plugin_configuration() {
            let config_str = String::from_utf8(config_bytes.to_vec()).unwrap_or_default();
            let config_json: Value = serde_json::from_str(&config_str).unwrap_or_default();

            // Parse OpenAPI paths
            let mut new_router = Router::new();
            if let Some(paths) = config_json.get("paths").and_then(Value::as_object) {
                for (path, _) in paths {
                    if let Err(e) = new_router.insert(path.clone(), path.clone()) {
                        info!("Failed to insert route: {e}");
                    }
                }
            }

            // 새로운 Router를 Arc로 감싸서 기존 것을 교체
            self.router = Arc::new(new_router);
            true
        } else {
            false
        }
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(OpenapiPathHttpContext {
            router: Arc::clone(&self.router), // Arc clone 사용
        }))
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }
}