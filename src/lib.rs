use log::{error, info, warn};
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde_json::Value;
use std::sync::Arc;
use matchit::Router;
use lru::LruCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;

// 플러그인 초기화
proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> {
        Box::new(OpenapiPathRootContext::default())
    });
}}

struct OpenapiPathRootContext {
    router: Arc<Router<String>>,
    cache: Arc<Mutex<LruCache<String, String>>>,
}

static DEFAULT_CACHE_SIZE: usize = 1024;

impl Default for OpenapiPathRootContext {
    fn default() -> Self {
        Self {
            router: Arc::new(Router::new()),
            cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap()))),
        }
    }
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
            cache: Arc::clone(&self.cache),
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

        let cache_size = config.get("cache_size")
            .and_then(Value::as_u64)
            .map(|size| size as usize)
            .unwrap_or(DEFAULT_CACHE_SIZE);

        let new_cache = LruCache::new(
            NonZeroUsize::new(cache_size)
                .unwrap_or(NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap())
        );

        self.router = Arc::new(new_router);
        self.cache = Arc::new(Mutex::new(new_cache));

        info!("Router configured successfully with {} paths and cache size {}", paths.len(), cache_size);
        Ok(())
    }
}

struct OpenapiPathHttpContext {
    router: Arc<Router<String>>,
    cache: Arc<Mutex<LruCache<String, String>>>,
}

impl Context for OpenapiPathHttpContext {}

impl HttpContext for OpenapiPathHttpContext {
    fn on_http_request_headers(&mut self, _num_headers: usize, _end_of_stream: bool) -> Action {
        let path = self.get_http_request_header(":path").unwrap_or_default();
        match self.process_request_path(&path) {
            Ok(matched_value) => {
                self.set_http_request_header("x-openapi-path", Some(&matched_value));
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
    fn process_request_path(&self, path: &str) -> Result<String, Box<dyn std::error::Error>> {
        {
            let mut cache = self.cache.lock();
            if let Some(cached_value) = cache.get(path) {
                return Ok(cached_value.clone());
            }
        }

        let result = match self.router.at(path) {
            Ok(matched) => matched.value.clone(),
            Err(e) => {
                warn!("Path '{}' not found in OpenAPI spec: {}", path, e);
                String::new()
            }
        };

        self.cache.lock().put(path.to_string(), result.clone());

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONFIG: &str = r#"{
        "cache_size": 2,
        "paths": {
            "/api/users/{id}": {},
            "/api/posts/{postId}/comments/{commentId}": {},
            "/api/items": {}
        }
    }"#;

    #[test]
    fn test_path_parameter_matching() {
        let mut root_ctx = OpenapiPathRootContext::default();
        root_ctx.configure_router_with_bytes(TEST_CONFIG.as_bytes().to_vec()).unwrap();

        let http_ctx = OpenapiPathHttpContext {
            router: Arc::clone(&root_ctx.router),
            cache: Arc::clone(&root_ctx.cache),
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
    fn test_path_caching() {
        let mut root_ctx = OpenapiPathRootContext::default();
        root_ctx.configure_router_with_bytes(TEST_CONFIG.as_bytes().to_vec()).unwrap();

        let http_ctx = OpenapiPathHttpContext {
            router: Arc::clone(&root_ctx.router),
            cache: Arc::clone(&root_ctx.cache),
        };

        let path = "/api/users/123";
        let first_result = http_ctx.process_request_path(path).unwrap();

        let cached_value = http_ctx.cache.lock().get(path).cloned();
        assert_eq!(cached_value, Some("/api/users/{id}".to_string()),
            "Path should be cached after first request");

        let second_result = http_ctx.process_request_path(path).unwrap();
        assert_eq!(first_result, second_result,
            "Cached result should match first result");
    }
}