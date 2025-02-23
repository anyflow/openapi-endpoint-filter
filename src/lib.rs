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

    // 필수로 붙여야. 없으면 `create_http_context()` 호출 시 hang 발생
    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
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

        let config = match String::from_utf8(config_bytes) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to convert config bytes to UTF-8 string: {}", e);
                return false;
            }
        };

        match self.configure_router(&config) {
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
}

impl OpenapiPathRootContext {
    fn configure_router(&mut self, config: &str) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Configuring router with config string of length: {} chars", config.len());
        let config: Value = serde_json::from_str(config)?;

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
        debug!("Getting the path from header");
        let path = self.get_http_request_header(":path").unwrap_or_default();

        debug!("Request path (without query): {}", path);
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
        let normalized_path = path.split('?').next().unwrap_or("");

        debug!("Checking if path exists in router: {}", normalized_path);
        match self.router.at(normalized_path) {
            Ok(matched) => {
                debug!("Path '{}' matched with value: {}", normalized_path, matched.value);
                Some(matched.value)
            }
            Err(e) => {
                warn!("Path '{}' not found in configuration: {}", normalized_path, e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // 테스트용 샘플 OpenAPI 경로 설정
    const TEST_CONFIG: &str = r#"{
        "paths": {
            "/api/users/{id}": {},
            "/api/posts/{postId}/comments/{commentId}": {},
            "/api/items": {}
        }
    }"#;

    /// 경로 매칭 테스트:
    /// - 기본 경로, 쿼리 문자열 포함 경로, trailing slash 포함 경로 테스트
    #[test]
    fn test_path_parameter_matching() {
        let mut root_ctx = OpenapiPathRootContext::default();
        root_ctx.configure_router(TEST_CONFIG).unwrap();

        let http_ctx = OpenapiPathHttpContext {
            router: Arc::clone(&root_ctx.router)
        };

        let test_cases = vec![
            // 기본 매칭 경로
            ("/api/items", Some("/api/items")),
            ("/api/users/123", Some("/api/users/{id}")),
            ("/api/posts/456/comments/789", Some("/api/posts/{postId}/comments/{commentId}")),
            // 쿼리 문자열 포함 경로
            ("/api/users/123?key=value", Some("/api/users/{id}")),
            ("/api/items?sort=asc&page=2", Some("/api/items")),
            ("/api/posts/456/comments/789?active=true", Some("/api/posts/{postId}/comments/{commentId}")),
            ("/api/items?filter=active&limit=10", Some("/api/items")),
            ("/api/posts/1/comments/2?a=b&c=d", Some("/api/posts/{postId}/comments/{commentId}")),
            // 복잡한 쿼리 문자열
            ("/api/users/456?key=value&nested[key]=val", Some("/api/users/{id}")),
            // trailing slash 포함 경로
            ("/api/users/123/", None),
            ("/api/items/", None),
            ("/api/posts/456/comments/789/", None),
            // trailing slash와 쿼리 문자열 조합
            ("/api/users/123/?key=value", None),
            ("/api/items/?sort=asc", None),
            // 매칭되지 않는 경로
            ("/api/users", None),
            ("/api/posts/456", None),
            ("/api/nonexistent", None),
        ];

        for (input_path, expected) in test_cases {
            let result = http_ctx.process_request_path(input_path);
            assert_eq!(result, expected,
                "Path '{}' should match '{:?}' but got '{:?}'", input_path, expected, result);
        }
    }

    /// 잘못된 JSON 설정 테스트
    #[test]
    fn test_invalid_json() {
        let mut context = OpenapiPathRootContext::default();
        let invalid_configs = vec![
            "{invalid json}",
            "[\"array\", \"instead\"]",
            "null",
        ];

        for config in invalid_configs {
            assert!(context.configure_router(config).is_err(),
                "Should fail to configure router with invalid JSON: {}", config);
        }
    }

    /// 'paths' 필드 누락 테스트
    #[test]
    fn test_missing_paths() {
        let mut context = OpenapiPathRootContext::default();
        let configs = vec![
            json!({}).to_string(),
            json!({"paths": "string"}).to_string(),
            json!({"paths": ["array"]}).to_string(),
        ];

        for config in configs {
            assert!(context.configure_router(&config).is_err(),
                "Should fail to configure router with missing or invalid 'paths': {}", config);
        }
    }

    /// 빈 경로 설정 테스트
    #[test]
    fn test_empty_paths() {
        let mut root_ctx = OpenapiPathRootContext::default();
        let config = json!({
            "paths": {}
        }).to_string();
        root_ctx.configure_router(&config).unwrap();

        let http_ctx = OpenapiPathHttpContext {
            router: Arc::clone(&root_ctx.router)
        };

        assert_eq!(http_ctx.process_request_path("/api/users/123"), None,
            "No paths should match with empty configuration");
    }
}