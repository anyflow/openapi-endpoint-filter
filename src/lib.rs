use log::{debug, error, info, warn};
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde_json::Value;
use std::sync::Arc;
use matchit::Router;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> {
        Box::new(OpenapiPathRootContext::default())
    });
}}

#[derive(Default)]
struct OpenapiPathRootContext {
    router: Arc<Router<String>>,
}

impl Context for OpenapiPathRootContext {}

impl RootContext for OpenapiPathRootContext {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool {
        info!("OpenAPI path filter initialized");
        true
    }

    fn on_configure(&mut self, _: usize) -> bool {
        debug!("Configuring openapi-path-filter");
        let config_bytes = match self.get_plugin_configuration() {
            Some(bytes) => bytes,
            None => {
                error!("No plugin configuration found");
                return false;
            }
        };

        match self.configure_router(config_bytes) {
            Ok(_) => {
                info!("Router configured successfully");
                true
            }
            Err(e) => {
                error!("Failed to configure router: {}", e);
                false
            }
        }
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        debug!("Creating HTTP context");
        Some(Box::new(OpenapiPathHttpContext {
            router: Arc::clone(&self.router),
        }))
    }

    fn get_type(&self) -> Option<ContextType> { // 반드시 필요. 없으면 create_http_context() 호출 시 오류 발생
        Some(ContextType::HttpContext)
    }
}

impl OpenapiPathRootContext {
    fn configure_router(&mut self, config_bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Configuring router with bytes: {} bytes", config_bytes.len());
        let config_str = String::from_utf8(config_bytes)?;
        let config: Value = serde_json::from_str(&config_str)?;

        let paths = config.get("paths")
            .and_then(Value::as_object)
            .ok_or("Invalid or missing 'paths' in configuration")?;

        let mut new_router = Router::new();
        for (path, _) in paths {
            debug!("Inserting route: {}", path);
            new_router.insert(path, path.clone())
                .map_err(|e| format!("Failed to insert route {}: {}", path, e))?;
        }

        self.router = Arc::new(new_router);
        info!("Router configured successfully with {} paths", paths.len());
        Ok(())
    }
}

#[derive(Default)]
struct OpenapiPathHttpContext {
    router: Arc<Router<String>>,
}

impl Context for OpenapiPathHttpContext {}

impl HttpContext for OpenapiPathHttpContext {
    fn on_http_request_headers(&mut self, _nheaders: usize, _end_of_stream: bool) -> Action {
        debug!("Processing HTTP request headers");
        let path = self.get_http_request_header(":path").unwrap_or_default();
        debug!("Request path: {}", path);
        match self.process_request_path(&path) {
            Some(matched_value) => {
                self.set_http_request_header("x-openapi-path", Some(matched_value));
            }
            None => {}
        }
        Action::Continue
    }
}

impl OpenapiPathHttpContext {
    fn process_request_path(&self, path: &str) -> Option<&str> {
        debug!("Checking if path exists in router: {}", path);
        match self.router.at(path) {
            Ok(matched) => {
                debug!("Path '{}' matched with value: {}", path, matched.value);
                Some(matched.value)
            }
            Err(e) => {
                warn!("Path '{}' not found in configuration: {}", path, e);
                None
            }
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONFIG: &str = r#"{
        "paths": {
            "/api/users/{id}": {},
            "/api/posts/{postId}/comments/{commentId}": {},
            "/api/items": {}
        }
    }"#;

    #[test]
    fn test_path_parameter_matching() {
        let mut root_ctx = OpenapiPathRootContext::default();
        root_ctx.configure_router(TEST_CONFIG.as_bytes().to_vec()).unwrap();

        let http_ctx = OpenapiPathHttpContext {
            router: Arc::clone(&root_ctx.router)
        };

        let test_cases = vec![
            ("/api/users/123", "/api/users/{id}"),
            ("/api/posts/456/comments/789", "/api/posts/{postId}/comments/{commentId}"),
            ("/api/items", "/api/items"),
        ];

        for (input_path, expected_match) in test_cases {
            let result = http_ctx.process_request_path(input_path).unwrap();
            assert_eq!(result, expected_match,
                "Path '{}' should match '{}'", input_path, expected_match);
        }
    }

    #[test]
    fn test_invalid_json() {
        let mut context = OpenapiPathRootContext::default();
        let invalid_config = b"{invalid json}".to_vec();

        assert!(context.configure_router(invalid_config).is_err());
    }

    #[test]
    fn test_missing_paths() {
        let mut context = OpenapiPathRootContext::default();
        let config = serde_json::json!({
            "other_field": "value"
        });
        let config_bytes = serde_json::to_vec(&config).unwrap();

        assert!(context.configure_router(config_bytes).is_err());
    }
}