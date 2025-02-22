use log::{error, info, warn};
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde_json::Value;
use std::sync::Arc;
use matchit::Router;

// 플러그인 초기화
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
        match self.configure_router() {
            Ok(_) => true,
            Err(e) => {
                error!("Failed to configure router: {}", e);
                false
            }
        }
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(OpenapiPathHttpContext {
            router: Arc::clone(&self.router),
        }))
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }
}

impl OpenapiPathRootContext {
    fn configure_router(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let config_bytes = self.get_plugin_configuration()
            .ok_or("No plugin configuration found")?;
        self.configure_router_with_bytes(config_bytes)
    }

    fn configure_router_with_bytes(&mut self, config_bytes: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        let config_str = String::from_utf8(config_bytes)?;
        let config: Value = serde_json::from_str(&config_str)?;

        let paths = config.get("paths")
            .and_then(Value::as_object)
            .ok_or("Invalid or missing 'paths' in configuration")?;

        let mut new_router = Router::new();
        for (path, _) in paths {
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
        let path = self.get_http_request_header(":path").unwrap_or_default();
        match self.process_request_path(&path) {
            Ok(matched_value) => {
                self.set_http_request_header("x-openapi-path", Some(matched_value));
                Action::Continue
            }
            Err(e) => {
                warn!("Error processing request path: {}", e);
                Action::Continue
            }
        }
    }
}

impl OpenapiPathHttpContext {
    /// Processes the request path against the router without relying on HttpContext methods.
    fn process_request_path(&self, path: &str) -> Result<&str, Box<dyn std::error::Error>> {
        match self.router.at(path) {
            Ok(matched) => Ok(matched.value),
            Err(e) => {
                warn!("Path '{}' not found in OpenAPI spec: {}", path, e);
                Ok("") // 기본값 반환
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_root_context() {
        let context = OpenapiPathRootContext::default();
        assert!(context.router.at("/").is_err()); // 기본 라우터는 비어있어야 함
    }

    #[test]
    fn test_valid_configuration() {
        let mut context = OpenapiPathRootContext::default();
        let config = serde_json::json!({
            "paths": {
                "/api/v1/users": {},
                "/api/v1/products/{id}": {}
            }
        });
        let config_bytes = serde_json::to_vec(&config).unwrap();

        assert!(context.configure_router_with_bytes(config_bytes).is_ok());
        assert!(context.router.at("/api/v1/users").is_ok());
        assert!(context.router.at("/api/v1/products/123").is_ok());
        assert!(context.router.at("/invalid/path").is_err());
    }

    #[test]
    fn test_invalid_json() {
        let mut context = OpenapiPathRootContext::default();
        let invalid_config = b"{invalid json}".to_vec();

        assert!(context.configure_router_with_bytes(invalid_config).is_err());
    }

    #[test]
    fn test_missing_paths() {
        let mut context = OpenapiPathRootContext::default();
        let config = serde_json::json!({
            "other_field": "value"
        });
        let config_bytes = serde_json::to_vec(&config).unwrap();

        assert!(context.configure_router_with_bytes(config_bytes).is_err());
    }

    #[test]
    fn test_path_parameter_matching() {
        let mut context = OpenapiPathRootContext::default();
        let config = serde_json::json!({
            "paths": {
                "/api/users/{id}": {},
                "/api/orders/{orderId}/items/{itemId}": {}
            }
        });
        let config_bytes = serde_json::to_vec(&config).unwrap();

        assert!(context.configure_router_with_bytes(config_bytes).is_ok());

        let match_result = context.router.at("/api/users/123").unwrap();
        assert_eq!(match_result.value, "/api/users/{id}");

        let match_result = context.router.at("/api/orders/456/items/789").unwrap();
        assert_eq!(match_result.value, "/api/orders/{orderId}/items/{itemId}");
    }
}