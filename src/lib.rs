use log::{debug, error, info};
use lru::LruCache;
use matchit::Router;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde_json::Value;
use std::cell::RefCell;
use std::num::NonZeroUsize;
use std::rc::Rc;

static DEFAULT_CACHE_SIZE: usize = 1024;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> {
        Box::new(OpenapiPathRoot::new())
    });
}}

struct OpenapiPathRoot {
    router: Rc<Router<(String, Rc<String>)>>, // (path_template, service_name)
    cache: Option<Rc<RefCell<LruCache<String, (String, Rc<String>)>>>>,
}

impl OpenapiPathRoot {
    fn new() -> Self {
        Self {
            router: Rc::new(Router::new()),
            cache: Some(Rc::new(RefCell::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap(),
            )))),
        }
    }
}

impl Context for OpenapiPathRoot {
    fn on_done(&mut self) -> bool {
        info!("[opf] openapi-path-filter terminated");
        true
    }
}

impl RootContext for OpenapiPathRoot {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool {
        info!("[opf] openapi-path-filter initialized");
        true
    }

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
        Some(Box::new(OpenapiPathFilter {
            router: Rc::clone(&self.router),
            cache: self.cache.as_ref().map(Rc::clone),
        }))
    }
}

impl OpenapiPathRoot {
    fn configure(&mut self, config: &Value) -> Result<(), Box<dyn std::error::Error>> {
        let size = config
            .get("cacheSize")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_CACHE_SIZE as u64);

        if size == 0 {
            self.cache = None;
        } else {
            let cache_size = NonZeroUsize::new(size as usize)
                .unwrap_or(NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap());
            self.cache = Some(Rc::new(RefCell::new(LruCache::new(cache_size))));
        }

        let services = config
            .get("services")
            .and_then(Value::as_array)
            .ok_or("Invalid or missing 'services' in configuration")?;

        let mut new_router = Router::new();
        for service in services {
            let service_name = service
                .get("name")
                .and_then(Value::as_str)
                .ok_or("Missing 'name' in service configuration")?;

            let paths = service
                .get("paths")
                .and_then(Value::as_object)
                .ok_or("Invalid or missing 'paths' in service configuration")?;

            for (path, _) in paths {
                debug!(
                    "[opf] Inserting route: {} for service: {}",
                    path, service_name
                );
                new_router.insert(path, (path.clone(), Rc::new(service_name.to_string())))?;
            }
        }
        self.router = Rc::new(new_router);

        info!(
            "[opf] Router configured successfully with {} services and cache size {}",
            services.len(),
            match &self.cache {
                Some(cache) => cache.borrow().cap().to_string(),
                None => "None".to_string(),
            }
        );
        Ok(())
    }
}

struct OpenapiPathFilter {
    router: Rc<Router<(String, Rc<String>)>>,
    cache: Option<Rc<RefCell<LruCache<String, (String, Rc<String>)>>>>,
}

impl Context for OpenapiPathFilter {}

impl HttpContext for OpenapiPathFilter {
    fn on_http_request_headers(&mut self, _nheaders: usize, _end_of_stream: bool) -> Action {
        debug!("[opf] Getting the path from header");
        let path = self.get_http_request_header(":path").unwrap_or_default();
        if let Some((matched_path, service_name)) = self.get_openapi_path(&path) {
            self.set_http_request_header("x-openapi-path", Some(&matched_path));
            self.set_http_request_header("x-service-name", Some(&service_name));
        }
        Action::Continue
    }
}

impl OpenapiPathFilter {
    fn get_openapi_path(&self, path: &str) -> Option<(String, Rc<String>)> {
        let normalized_path = path.split('?').next().unwrap_or("").to_string();
        if let Some(cache) = &self.cache {
            let mut cache = cache.borrow_mut();
            if let Some((matched_path, service_name)) = cache.get(&normalized_path) {
                debug!(
                    "[opf] Cache hit for path: {}, value: ({}, {})",
                    normalized_path, matched_path, service_name
                );
                return Some((matched_path.clone(), Rc::clone(&service_name)));
            }
        }
        debug!(
            "[opf] Cache miss or cache disabled for path: {}",
            normalized_path
        );
        match self.router.at(&normalized_path) {
            Ok(matched) => {
                let (matched_path, service_name) = matched.value.clone();
                if let Some(cache) = &self.cache {
                    cache.borrow_mut().put(
                        normalized_path,
                        (matched_path.clone(), Rc::clone(&service_name)),
                    );
                }
                debug!(
                    "[opf] {} matched and cached with {}, {}",
                    path, service_name, matched_path
                );
                Some((matched_path, service_name))
            }
            Err(_) => {
                debug!("[opf] No match found for path: {}", normalized_path);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_CONFIG: &str = r#"{
        "cacheSize": 5,
        "services": [
            {
                "name": "dockebi",
                "paths": {
                    "/dockebi/v1/stuff": {},
                    "/dockebi/v1/stuff/{id_}": {},
                    "/dockebi/v1/stuff/{id_}/child/{child_id}/hello": {}
                }
            },
            {
                "name": "userservice",
                "paths": {
                    "/users": {},
                    "/users/{id}": {},
                    "/users/{id}/profile": {}
                }
            },
            {
                "name": "productservice",
                "paths": {
                    "/products": {},
                    "/products/{product_id}": {},
                    "/categories/{category_id}/products": {}
                }
            }
        ]
    }"#;

    const MINIMAL_CONFIG: &str = r#"{
        "services": [
            {
                "name": "minimal",
                "paths": {
                    "/test": {}
                }
            }
        ]
    }"#;

    const DISABLE_CACHE_CONFIG: &str = r#"{
        "cacheSize": 0,
        "services": [
            {
                "name": "nocache",
                "paths": {
                    "/api/v1/test": {}
                }
            }
        ]
    }"#;

    #[test]
    fn test_basic_path_and_service_matching() {
        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiPathFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
        };

        let test_cases = vec![
            (
                "/dockebi/v1/stuff",
                Some((
                    "/dockebi/v1/stuff".to_string(),
                    Rc::new("dockebi".to_string()),
                )),
            ),
            (
                "/dockebi/v1/stuff/123",
                Some((
                    "/dockebi/v1/stuff/{id_}".to_string(),
                    Rc::new("dockebi".to_string()),
                )),
            ),
            (
                "/dockebi/v1/stuff/123/child/456/hello",
                Some((
                    "/dockebi/v1/stuff/{id_}/child/{child_id}/hello".to_string(),
                    Rc::new("dockebi".to_string()),
                )),
            ),
            (
                "/dockebi/v1/stuff/123?key=value",
                Some((
                    "/dockebi/v1/stuff/{id_}".to_string(),
                    Rc::new("dockebi".to_string()),
                )),
            ),
            ("/dockebi/v1/other", None),
            (
                "/users",
                Some(("/users".to_string(), Rc::new("userservice".to_string()))),
            ),
            (
                "/users/42",
                Some((
                    "/users/{id}".to_string(),
                    Rc::new("userservice".to_string()),
                )),
            ),
            (
                "/users/42/profile",
                Some((
                    "/users/{id}/profile".to_string(),
                    Rc::new("userservice".to_string()),
                )),
            ),
            (
                "/products",
                Some((
                    "/products".to_string(),
                    Rc::new("productservice".to_string()),
                )),
            ),
            (
                "/products/xyz123",
                Some((
                    "/products/{product_id}".to_string(),
                    Rc::new("productservice".to_string()),
                )),
            ),
            (
                "/categories/furniture/products",
                Some((
                    "/categories/{category_id}/products".to_string(),
                    Rc::new("productservice".to_string()),
                )),
            ),
            ("/unknownpath", None),
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
    fn test_path_normalization() {
        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiPathFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
        };

        let test_cases = vec![
            (
                "/users/42?sortBy=name&order=asc",
                Some((
                    "/users/{id}".to_string(),
                    Rc::new("userservice".to_string()),
                )),
            ),
            (
                "/products?category=electronics&inStock=true",
                Some((
                    "/products".to_string(),
                    Rc::new("productservice".to_string()),
                )),
            ),
            (
                "/categories/books/products?featured=true&limit=10",
                Some((
                    "/categories/{category_id}/products".to_string(),
                    Rc::new("productservice".to_string()),
                )),
            ),
        ];

        for (input_path, expected) in test_cases {
            let result = http_ctx.get_openapi_path(input_path);
            assert_eq!(
                result, expected,
                "Path with query params '{}' should match '{:?}' but got '{:?}'",
                input_path, expected, result
            );
        }
    }

    #[test]
    fn test_cache_behavior() {
        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiPathFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
        };

        // First access - should go to the router
        let path1 = "/dockebi/v1/stuff/123";
        let result1 = http_ctx.get_openapi_path(path1);
        assert_eq!(
            result1,
            Some((
                "/dockebi/v1/stuff/{id_}".to_string(),
                Rc::new("dockebi".to_string())
            ))
        );

        // Second access - should be served from cache
        let result2 = http_ctx.get_openapi_path(path1);
        assert_eq!(
            result2,
            Some((
                "/dockebi/v1/stuff/{id_}".to_string(),
                Rc::new("dockebi".to_string())
            ))
        );

        // Verify cache state
        if let Some(cache) = &root_ctx.cache {
            assert_eq!(cache.borrow().len(), 1);
            assert_eq!(cache.borrow().cap().get(), 5);

            // Check that the item is in the cache
            assert!(cache.borrow().contains(&path1.to_string()));
        } else {
            panic!("Cache should be enabled");
        }

        // Fill up the cache and test LRU behavior
        http_ctx.get_openapi_path("/users"); // 2nd item
        http_ctx.get_openapi_path("/users/1"); // 3rd item
        http_ctx.get_openapi_path("/products"); // 4th item
        http_ctx.get_openapi_path("/products/123"); // 5th item

        // This should evict the oldest item (path1)
        http_ctx.get_openapi_path("/categories/tech/products");

        if let Some(cache) = &root_ctx.cache {
            assert_eq!(cache.borrow().len(), 5);
            // The oldest item should be evicted
            assert!(!cache.borrow().contains(&path1.to_string()));
        } else {
            panic!("Cache should be enabled");
        }
    }

    #[test]
    fn test_default_cache_size() {
        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx
            .configure(&serde_json::from_str(MINIMAL_CONFIG).unwrap())
            .unwrap();

        if let Some(cache) = &root_ctx.cache {
            assert_eq!(cache.borrow().cap().get(), DEFAULT_CACHE_SIZE);
        } else {
            panic!("Cache should be enabled by default");
        }
    }

    #[test]
    fn test_disabled_cache() {
        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx
            .configure(&serde_json::from_str(DISABLE_CACHE_CONFIG).unwrap())
            .unwrap();

        assert!(
            root_ctx.cache.is_none(),
            "Cache should be disabled when cache_size is 0"
        );

        let http_ctx = OpenapiPathFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
        };

        // The path should still match correctly even with cache disabled
        let path = "/api/v1/test";
        let result = http_ctx.get_openapi_path(path);
        assert_eq!(
            result,
            Some(("/api/v1/test".to_string(), Rc::new("nocache".to_string())))
        );
    }

    #[test]
    fn test_complex_path_patterns() {
        let config = json!({
            "services": [
                {
                    "name": "complexapi",
                    "paths": {
                        "/api/v1/resources/{resource_id}/subresources/{subresource_id}/items/{item_id}": {},
                        "/api/v1/users/{user_id}/orders/{order_id}/items/{item_id}/tracking": {},
                        "/{tenant_id}/dashboard": {}
                    }
                }
            ]
        });

        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx.configure(&config).unwrap();

        let http_ctx = OpenapiPathFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
        };

        let test_cases = vec![
            (
                "/api/v1/resources/r123/subresources/sub456/items/i789",
                Some((
                    "/api/v1/resources/{resource_id}/subresources/{subresource_id}/items/{item_id}"
                        .to_string(),
                    Rc::new("complexapi".to_string()),
                )),
            ),
            (
                "/api/v1/users/u123/orders/o456/items/i789/tracking",
                Some((
                    "/api/v1/users/{user_id}/orders/{order_id}/items/{item_id}/tracking"
                        .to_string(),
                    Rc::new("complexapi".to_string()),
                )),
            ),
            (
                "/tenant1/dashboard",
                Some((
                    "/{tenant_id}/dashboard".to_string(),
                    Rc::new("complexapi".to_string()),
                )),
            ),
            ("/api/v1/resources/r123/something_else", None),
        ];

        for (input_path, expected) in test_cases {
            let result = http_ctx.get_openapi_path(input_path);
            assert_eq!(
                result, expected,
                "Complex path '{}' should match '{:?}' but got '{:?}'",
                input_path, expected, result
            );
        }
    }

    #[test]
    fn test_invalid_configurations() {
        let test_cases = vec![
            // Missing services array
            (
                json!({
                    "cacheSize": 10
                }),
                "Invalid or missing 'services' in configuration",
            ),
            // Service without name
            (
                json!({
                    "services": [
                        {
                            "paths": {
                                "/test": {}
                            }
                        }
                    ]
                }),
                "Missing 'name' in service configuration",
            ),
            // Service without paths
            (
                json!({
                    "services": [
                        {
                            "name": "test"
                        }
                    ]
                }),
                "Invalid or missing 'paths' in service configuration",
            ),
            // Invalid paths type
            (
                json!({
                    "services": [
                        {
                            "name": "test",
                            "paths": "invalid"
                        }
                    ]
                }),
                "Invalid or missing 'paths' in service configuration",
            ),
        ];

        for (config, expected_error) in test_cases {
            let mut root_ctx = OpenapiPathRoot::new();
            let result = root_ctx.configure(&config);

            assert!(result.is_err(), "Configuration should fail: {:?}", config);
            let error = result.err().unwrap();
            assert!(
                error.to_string().contains(expected_error),
                "Error message should contain '{}', but got '{}'",
                expected_error,
                error
            );
        }
    }

    #[test]
    fn test_multiple_service_overlapping_routes() {
        let config = json!({
            "services": [
                {
                    "name": "service1",
                    "paths": {
                        "/api/v1/shared": {},
                        "/api/v1/service1/specific": {}
                    }
                },
                {
                    "name": "service2",
                    "paths": {
                        "/api/v1/shared/{id}": {},
                        "/api/v1/service2/specific": {}
                    }
                }
            ]
        });

        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx.configure(&config).unwrap();

        let http_ctx = OpenapiPathFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
        };

        // For exact path matches, the first service should win
        assert_eq!(
            http_ctx.get_openapi_path("/api/v1/shared"),
            Some((
                "/api/v1/shared".to_string(),
                Rc::new("service1".to_string())
            ))
        );

        // For parameterized paths, the match should work correctly
        assert_eq!(
            http_ctx.get_openapi_path("/api/v1/shared/123"),
            Some((
                "/api/v1/shared/{id}".to_string(),
                Rc::new("service2".to_string())
            ))
        );

        // Service-specific paths should go to the correct service
        assert_eq!(
            http_ctx.get_openapi_path("/api/v1/service1/specific"),
            Some((
                "/api/v1/service1/specific".to_string(),
                Rc::new("service1".to_string())
            ))
        );

        assert_eq!(
            http_ctx.get_openapi_path("/api/v1/service2/specific"),
            Some((
                "/api/v1/service2/specific".to_string(),
                Rc::new("service2".to_string())
            ))
        );
    }
}
