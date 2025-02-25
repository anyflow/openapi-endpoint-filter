use log::{debug, error, info};
use lru::LruCache;
use matchit::Router;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde_json::Value;
use std::num::NonZeroUsize;
use std::rc::Rc;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> {
        Box::new(OpenapiPathRootContext::new())
    });
}}

struct OpenapiPathRootContext {
    router: Rc<Router<String>>,
    cache: Option<LruCache<String, String>>,
}

impl OpenapiPathRootContext {
    fn new() -> Self {
        Self {
            router: Rc::new(Router::new()),
            cache: None,
        }
    }
}

impl Context for OpenapiPathRootContext {
    fn on_done(&mut self) -> bool {
        info!("[opf] openapi-path-filter terminated");
        true
    }
}

impl RootContext for OpenapiPathRootContext {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool {
        info!("[opf] openapi-path-filter initialized");
        true
    }

    // 필수로 붙여야. 없으면 `create_http_context()` 호출 시 hang 발생
    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn on_configure(&mut self, _: usize) -> bool {
        debug!("[opf] Configuring openapi-path-filter");
        let config_bytes = match self.get_plugin_configuration() {
            Some(bytes) => bytes,
            None => {
                error!("[opf] No plugin configuration found");
                return false;
            }
        };

        let config_str = match String::from_utf8(config_bytes) {
            Ok(s) => s,
            Err(e) => {
                error!("[opf] Failed to convert bytes to UTF-8 string: {}", e);
                return false;
            }
        };

        let config: Value = match serde_json::from_str(&config_str) {
            Ok(v) => v,
            Err(e) => {
                error!("[opf] Failed to parse JSON configuration: {}", e);
                return false;
            }
        };

        match self.configure(&config) {
            Ok(_) => {
                info!("[opf] Configuration successful");
                true
            }
            Err(e) => {
                error!("[opf] Configuration failed: {}", e);
                false
            }
        }
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        debug!("[opf] Creating HTTP context");
        Some(Box::new(OpenapiPathHttpContext {
            router: Rc::clone(&self.router),
            cache: self.cache.as_ref().unwrap().clone(), // Clone으로 복사
        }))
    }
}

impl OpenapiPathRootContext {
    fn configure(&mut self, config: &Value) -> Result<(), Box<dyn std::error::Error>> {
        let cache_size = config
            .get("cache_size")
            .and_then(Value::as_u64)
            .unwrap_or(1024) as usize;
        let cache_size =
            NonZeroUsize::new(cache_size).ok_or("cache_size must be greater than 0")?;
        self.cache = Some(LruCache::new(cache_size));

        let paths = config
            .get("paths")
            .and_then(Value::as_object)
            .ok_or("Invalid or missing 'paths' in configuration")?;
        let mut new_router = Router::new();
        for (path, _) in paths {
            debug!("[opf] Inserting route: {}", path);
            new_router.insert(path, path.clone())?;
        }
        self.router = Rc::new(new_router);
        info!(
            "[opf] Router configured successfully with {} paths",
            paths.len()
        );
        Ok(())
    }
}

struct OpenapiPathHttpContext {
    router: Rc<Router<String>>,
    cache: LruCache<String, String>,
}

impl Context for OpenapiPathHttpContext {}

impl HttpContext for OpenapiPathHttpContext {
    fn on_http_request_headers(&mut self, _nheaders: usize, _end_of_stream: bool) -> Action {
        debug!("[opf] Getting the path from header");
        let path = self.get_http_request_header(":path").unwrap_or_default();
        if let Some(matched_value) = self.get_openapi_path(&path) {
            self.set_http_request_header("x-openapi-path", Some(&matched_value));
        }
        Action::Continue
    }
}

impl OpenapiPathHttpContext {
    fn get_openapi_path(&mut self, path: &str) -> Option<String> {
        let normalized_path = path.split('?').next().unwrap_or("").to_string();
        if let Some(matched_value) = self.cache.get(&normalized_path) {
            debug!(
                "[opf] Cache hit for path: {}, value: {}",
                normalized_path, matched_value
            );
            Some(matched_value.clone())
        } else {
            debug!("[opf] Cache miss for path: {}", normalized_path);
            match self.router.at(&normalized_path) {
                Ok(matched) => {
                    let matched_value = matched.value.clone();
                    self.cache.put(normalized_path, matched_value.clone());
                    Some(matched_value)
                }
                Err(_) => {
                    debug!("[opf] No match found for path: {}", normalized_path);
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_CONFIG: &str = r#"{
        "cache_size": 1024,
        "paths": {
            "/api/users/{id}": {},
            "/api/posts/{postId}/comments/{commentId}": {},
            "/api/items": {}
        }
    }"#;

    #[test]
    fn test_path_parameter_matching() {
        let mut root_ctx = OpenapiPathRootContext::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let mut http_ctx = OpenapiPathHttpContext {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.unwrap().clone(),
        };

        let test_cases = vec![
            ("/api/items", Some("/api/items".to_string())),
            ("/api/users/123", Some("/api/users/{id}".to_string())),
            (
                "/api/posts/456/comments/789",
                Some("/api/posts/{postId}/comments/{commentId}".to_string()),
            ),
            (
                "/api/users/123?key=value",
                Some("/api/users/{id}".to_string()),
            ),
            ("/api/items?sort=asc&page=2", Some("/api/items".to_string())),
            (
                "/api/posts/456/comments/789?active=true",
                Some("/api/posts/{postId}/comments/{commentId}".to_string()),
            ),
            (
                "/api/items?filter=active&limit=10",
                Some("/api/items".to_string()),
            ),
            (
                "/api/posts/1/comments/2?a=b&c=d",
                Some("/api/posts/{postId}/comments/{commentId}".to_string()),
            ),
            (
                "/api/users/456?key=value&nested[key]=val",
                Some("/api/users/{id}".to_string()),
            ),
            ("/api/users/123/", None),
            ("/api/items/", None),
            ("/api/posts/456/comments/789/", None),
            ("/api/users/123/?key=value", None),
            ("/api/items/?sort=asc", None),
            ("/api/users", None),
            ("/api/posts/456", None),
            ("/api/nonexistent", None),
        ];

        for (input_path, expected) in test_cases {
            let result = http_ctx.get_openapi_path(input_path);
            assert_eq!(
                result, expected,
                "Path '{}' should match '{:?}' but got '{:?}'",
                input_path, expected, result
            );
        }
    }

    #[test]
    fn test_invalid_config() {
        let mut context = OpenapiPathRootContext::new();
        let invalid_configs = vec![
            json!({}),
            json!({"paths": "string"}),
            json!({"paths": ["array"]}),
            json!({"cache_size": "not a number"}),
        ];

        for config in invalid_configs {
            assert!(
                context.configure(&config).is_err(),
                "Should fail to configure with invalid config: {}",
                config
            );
        }
    }

    #[test]
    fn test_empty_paths() {
        let mut root_ctx = OpenapiPathRootContext::new();
        let config = json!({
            "cache_size": 1024,
            "paths": {}
        })
        .to_string();
        root_ctx
            .configure(&serde_json::from_str(&config).unwrap())
            .unwrap();

        let mut http_ctx = OpenapiPathHttpContext {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.unwrap().clone(),
        };

        assert_eq!(
            http_ctx.get_openapi_path("/api/users/123"),
            None,
            "No paths should match with empty configuration"
        );
    }

    // #[test]
    // fn test_cache_behavior() {
    //     let mut root_ctx = OpenapiPathRootContext::new();
    //     let config = json!({
    //         "cache_size": 2,
    //         "paths": {
    //             "/api/users/{id}": {},
    //             "/api/items": {}
    //         }
    //     })
    //     .to_string();
    //     root_ctx
    //         .configure(&serde_json::from_str(&config).unwrap())
    //         .unwrap();

    //     let mut http_ctx1 = OpenapiPathHttpContext {
    //         router: Rc::clone(&root_ctx.router),
    //         cache: root_ctx.cache.as_ref().unwrap().clone(),
    //     };

    //     let mut http_ctx2 = OpenapiPathHttpContext {
    //         router: Rc::clone(&root_ctx.router),
    //         cache: root_ctx.cache.as_ref().unwrap().clone(),
    //     };

    //     let path1 = "/api/users/123";
    //     let result1 = http_ctx1.get_openapi_path(path1);
    //     assert_eq!(result1, Some("/api/users/{id}".to_string()));

    //     let result2 = http_ctx2.get_openapi_path(path1);
    //     assert_eq!(result2, Some("/api/users/{id}".to_string()));

    //     assert_eq!(http_ctx1.cache.len(), 1);

    //     let path2 = "/api/items";
    //     let result3 = http_ctx1.get_openapi_path(path2);
    //     assert_eq!(result3, Some("/api/items".to_string()));

    //     assert_eq!(http_ctx1.cache.len(), 2);

    //     let path3 = "/api/users/456";
    //     let result4 = http_ctx2.get_openapi_path(path3);
    //     assert_eq!(result4, Some("/api/users/{id}".to_string()));

    //     assert!(http_ctx2.cache.contains(&path2.to_string()));
    //     assert!(http_ctx2.cache.contains(&path3.to_string()));
    //     assert!(!http_ctx2.cache.contains(&path1.to_string()));
    // }
}
