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
        Box::new(OpenapiEndpointRoot::new())
    });
}}

struct OpenapiEndpointRoot {
    router: Rc<Router<(String, Rc<String>)>>, // (path_template, service_name)
    cache: Option<Rc<RefCell<LruCache<String, (String, Rc<String>)>>>>,
    preserve_existing_headers: bool,
    config_error: Option<String>,
}

impl OpenapiEndpointRoot {
    fn new() -> Self {
        Self {
            router: Rc::new(Router::new()),
            cache: Some(Rc::new(RefCell::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap(),
            )))),
            preserve_existing_headers: true,
            config_error: None,
        }
    }
}

impl Context for OpenapiEndpointRoot {
    fn on_done(&mut self) -> bool {
        info!("[oef] openapi-endpoint-filter terminated");
        true
    }
}

impl RootContext for OpenapiEndpointRoot {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool {
        info!("[oef] openapi-endpoint-filter initialized");
        true
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn on_configure(&mut self, _: usize) -> bool {
        debug!("[oef] Configuring openapi-endpoint-filter");
        let config_bytes = match self.get_plugin_configuration() {
            Some(bytes) => bytes,
            None => {
                error!("[oef] (ERR_NO_CONFIG) No plugin configuration found. Bypassing filter.");
                self.config_error = Some("ERR_NO_CONFIG".to_string());
                return true;
            }
        };

        let config_str = match String::from_utf8(config_bytes) {
            Ok(s) => s,
            Err(e) => {
                error!(
                    "[oef] (ERR_UTF8) Failed to convert bytes to UTF-8: {}. Bypassing filter.",
                    e
                );
                self.config_error = Some("ERR_UTF8".to_string());
                return true;
            }
        };

        let config: Value = match serde_json::from_str(&config_str) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "[oef] (ERR_JSON) Failed to parse JSON: {}. Bypassing filter.",
                    e
                );
                self.config_error = Some("ERR_JSON".to_string());
                return true;
            }
        };

        match self.configure(&config) {
            Ok(_) => {
                info!("[oef] ✅ Configuration successful");
                self.config_error = None;
            }
            Err(e) => {
                error!("[oef] ❌ (ERR_PARSE) Configuration failed: {}", e);
                error!("[oef] ⚠️  All requests will bypass filter (no metrics collected)");
                self.config_error = Some("ERR_PARSE".to_string());
            }
        }
        true
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        debug!("[oef] Creating HTTP context");
        Some(Box::new(OpenapiEndpointFilter {
            router: Rc::clone(&self.router),
            cache: self.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: self.preserve_existing_headers,
            config_error: self.config_error.clone(),
        }))
    }
}

impl OpenapiEndpointRoot {
    fn configure(&mut self, config: &Value) -> Result<(), Box<dyn std::error::Error>> {
        // === Phase 1: Parse and validate (no mutations to self) ===

        let preserve_existing_headers = config
            .get("preserveExistingHeaders")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let cache_size = config
            .get("cacheSize")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_CACHE_SIZE as u64);

        let services = config
            .get("services")
            .and_then(Value::as_array)
            .ok_or("Invalid or missing 'services' in configuration")?;

        if services.is_empty() {
            return Err("Services array cannot be empty".into());
        }

        // === Phase 2: Build new router (may fail, but self is untouched) ===

        let mut new_router = Router::new();
        for service in services {
            let service_name = service
                .get("name")
                .and_then(Value::as_str)
                .ok_or("Missing 'name' in service configuration")?;

            if service_name.is_empty() {
                return Err("Service name cannot be empty".into());
            }

            let paths = service
                .get("paths")
                .and_then(Value::as_object)
                .ok_or("Invalid or missing 'paths' in service configuration")?;

            if paths.is_empty() {
                return Err(format!("Service '{}' has no paths", service_name).into());
            }

            for (path, _) in paths {
                // Validate path format
                if !path.starts_with('/') {
                    return Err(format!("Path must start with '/': {}", path).into());
                }
                if path.len() > 1024 {
                    return Err(format!("Path too long (max 1024): {}", path).into());
                }
                if path.contains('\0') {
                    return Err(format!("Path contains null character: {}", path).into());
                }
                if path.contains(' ') {
                    return Err(format!("Path contains space (use %20): {}", path).into());
                }
                if path.contains('\n') || path.contains('\r') {
                    return Err(format!("Path contains newline character: {}", path).into());
                }

                debug!(
                    "[oef] Inserting route: {} for service: {}",
                    path, service_name
                );

                // Insert route with better error message for duplicates
                if let Err(e) = new_router.insert(path, (path.clone(), Rc::new(service_name.to_string()))) {
                    return Err(format!(
                        "Duplicate or conflicting route '{}' in service '{}': {}",
                        path, service_name, e
                    )
                    .into());
                }
            }
        }

        // === Phase 3: Build new cache ===

        let new_cache = if cache_size == 0 {
            None
        } else {
            let size = NonZeroUsize::new(cache_size as usize)
                .unwrap_or(NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap());
            Some(Rc::new(RefCell::new(LruCache::new(size))))
        };

        // === Phase 4: Apply all changes atomically ===
        // All validations passed, now we can safely update self

        self.router = Rc::new(new_router);
        self.cache = new_cache;
        self.preserve_existing_headers = preserve_existing_headers;

        info!(
            "[oef] ✅ Router configured successfully with {} services and cache size {}",
            services.len(),
            match &self.cache {
                Some(cache) => cache.borrow().cap().to_string(),
                None => "0 (disabled)".to_string(),
            }
        );
        Ok(())
    }
}

struct OpenapiEndpointFilter {
    router: Rc<Router<(String, Rc<String>)>>,
    cache: Option<Rc<RefCell<LruCache<String, (String, Rc<String>)>>>>,
    preserve_existing_headers: bool,
    config_error: Option<String>,
}

impl Context for OpenapiEndpointFilter {}

impl HttpContext for OpenapiEndpointFilter {
    fn on_http_request_headers(&mut self, _nheaders: usize, _end_of_stream: bool) -> Action {
        if let Some(code) = &self.config_error {
            debug!("[oef] ({}) Bypassing due to config error", code);

            // Set metric headers for monitoring config errors
            self.set_http_request_header("x-api-endpoint", Some("config-error"));
            self.set_http_request_header("x-service-name", Some("config-error"));
            self.set_http_request_header("x-path-template", Some("config-error"));

            return Action::Continue;
        }

        debug!("[oef] Getting the path from header");
        let path = self.get_http_request_header(":path").unwrap_or_default();
        let method = self
            .get_http_request_header(":method")
            .unwrap_or("unknown".to_string());
        let (path_template, service_name) = self
            .get_path_template(&path)
            .map(|(matched_path, service_name)| (matched_path, service_name))
            .unwrap_or(("unknown".to_string(), Rc::new("unknown".to_string())));

        if !self.preserve_existing_headers
            || self.get_http_request_header("x-service-name").is_none()
        {
            self.set_http_request_header("x-service-name", Some(&service_name));
        }
        if !self.preserve_existing_headers
            || self.get_http_request_header("x-path-template").is_none()
        {
            self.set_http_request_header("x-path-template", Some(&path_template));
        }
        if !self.preserve_existing_headers
            || self.get_http_request_header("x-api-endpoint").is_none()
        {
            self.set_http_request_header(
                "x-api-endpoint",
                Some(&if method == "unknown" && path_template == "unknown" {
                    "unknown".to_string()
                } else {
                    format!("{} {}", method, path_template)
                }),
            );
        }

        Action::Continue
    }
}

impl OpenapiEndpointFilter {
    fn normalize_path(path: &str) -> String {
        // Remove query string and fragment
        let without_query = path.split('?').next().unwrap_or("");
        let without_fragment = without_query.split('#').next().unwrap_or("");

        // Split by '/', filter out empty segments (removes duplicate slashes and trailing slash)
        let segments: Vec<&str> = without_fragment
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        // Handle root path
        if segments.is_empty() {
            return "/".to_string();
        }

        // Reconstruct path with single slashes only
        format!("/{}", segments.join("/"))
    }

    fn get_path_template(&self, path: &str) -> Option<(String, Rc<String>)> {
        let normalized_path = Self::normalize_path(path);
        if let Some(cache) = &self.cache {
            let mut cache = cache.borrow_mut();
            if let Some((matched_path, service_name)) = cache.get(&normalized_path) {
                debug!(
                    "[oef] Cache hit for path: {}, value: ({}, {})",
                    normalized_path, matched_path, service_name
                );
                return Some((matched_path.clone(), Rc::clone(&service_name)));
            }
        }
        debug!(
            "[oef] Cache miss or cache disabled for path: {}",
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
                    "[oef] {} matched and cached with {}, {}",
                    path, service_name, matched_path
                );
                Some((matched_path, service_name))
            }
            Err(_) => {
                debug!("[oef] No match found for path: {}", normalized_path);
                None
            }
        }
    }
}

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
        let mut root_ctx = OpenapiEndpointRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiEndpointFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: true,
            config_error: None,
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
            let result = http_ctx.get_path_template(input_path);
            assert_eq!(
                result, expected,
                "Path '{}' should match '{:?}' but got '{:?}'",
                input_path, expected, result
            );
        }
    }

    #[test]
    fn test_path_normalization() {
        let mut root_ctx = OpenapiEndpointRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiEndpointFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: true,
            config_error: None,
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
            let result = http_ctx.get_path_template(input_path);
            assert_eq!(
                result, expected,
                "Path with query params '{}' should match '{:?}' but got '{:?}'",
                input_path, expected, result
            );
        }
    }

    #[test]
    fn test_advanced_path_normalization() {
        let test_cases = vec![
            // Trailing slash
            ("/users/", "/users"),
            ("/products/", "/products"),
            // Root path should remain as /
            ("/", "/"),
            // Fragment removal
            ("/users#section", "/users"),
            ("/products#top", "/products"),
            // Duplicate slashes
            ("/users//profile", "/users/profile"),
            ("//users", "/users"),
            ("/users///profile///", "/users/profile"),
            ("///users///profile///", "/users/profile"),
            // Combined: query + fragment + trailing slash
            ("/users/?query=1#section", "/users"),
            ("/products?a=1&b=2#top", "/products"),
            // Combined: duplicate slashes + query + fragment
            ("//users//profile//?key=value#section", "/users/profile"),
            // Complex real-world cases
            ("/api//v1///users/{id}/?format=json#details", "/api/v1/users/{id}"),
            ("/dockebi/v1/stuff/{id_}//child/{child_id}/hello/?test=1", "/dockebi/v1/stuff/{id_}/child/{child_id}/hello"),
        ];

        for (input, expected) in test_cases {
            let result = OpenapiEndpointFilter::normalize_path(input);
            assert_eq!(
                result, expected,
                "normalize_path('{}') should return '{}' but got '{}'",
                input, expected, result
            );
        }
    }

    #[test]
    fn test_normalized_path_matching() {
        let mut root_ctx = OpenapiEndpointRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiEndpointFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: true,
            config_error: None,
        };

        // Test that normalized paths match correctly
        let test_cases = vec![
            // Trailing slash should match
            (
                "/users/",
                Some(("/users".to_string(), Rc::new("userservice".to_string()))),
            ),
            // Fragment should be ignored
            (
                "/users#section",
                Some(("/users".to_string(), Rc::new("userservice".to_string()))),
            ),
            // Duplicate slashes should match
            (
                "/users//42",
                Some((
                    "/users/{id}".to_string(),
                    Rc::new("userservice".to_string()),
                )),
            ),
            // Combined normalization
            (
                "/users/42//profile/?query=1#section",
                Some((
                    "/users/{id}/profile".to_string(),
                    Rc::new("userservice".to_string()),
                )),
            ),
            // Root path
            ("/", None),
        ];

        for (input_path, expected) in test_cases {
            let result = http_ctx.get_path_template(input_path);
            assert_eq!(
                result, expected,
                "Path '{}' should match '{:?}' but got '{:?}'",
                input_path, expected, result
            );
        }
    }

    #[test]
    fn test_cache_behavior() {
        let mut root_ctx = OpenapiEndpointRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiEndpointFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: true,
            config_error: None,
        };

        // First access - should go to the router
        let path1 = "/dockebi/v1/stuff/123";
        let result1 = http_ctx.get_path_template(path1);
        assert_eq!(
            result1,
            Some((
                "/dockebi/v1/stuff/{id_}".to_string(),
                Rc::new("dockebi".to_string())
            ))
        );

        // Second access - should be served from cache
        let result2 = http_ctx.get_path_template(path1);
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
        http_ctx.get_path_template("/users"); // 2nd item
        http_ctx.get_path_template("/users/1"); // 3rd item
        http_ctx.get_path_template("/products"); // 4th item
        http_ctx.get_path_template("/products/123"); // 5th item

        // This should evict the oldest item (path1)
        http_ctx.get_path_template("/categories/tech/products");

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
        let mut root_ctx = OpenapiEndpointRoot::new();
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
        let mut root_ctx = OpenapiEndpointRoot::new();
        root_ctx
            .configure(&serde_json::from_str(DISABLE_CACHE_CONFIG).unwrap())
            .unwrap();

        assert!(
            root_ctx.cache.is_none(),
            "Cache should be disabled when cache_size is 0"
        );

        let http_ctx = OpenapiEndpointFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: true,
            config_error: None,
        };

        // The path should still match correctly even with cache disabled
        let path = "/api/v1/test";
        let result = http_ctx.get_path_template(path);
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

        let mut root_ctx = OpenapiEndpointRoot::new();
        root_ctx.configure(&config).unwrap();

        let http_ctx = OpenapiEndpointFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: true,
            config_error: None,
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
            let result = http_ctx.get_path_template(input_path);
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
            // Empty services array
            (
                json!({
                    "services": []
                }),
                "Services array cannot be empty",
            ),
            // Empty service name
            (
                json!({
                    "services": [
                        {
                            "name": "",
                            "paths": {
                                "/test": {}
                            }
                        }
                    ]
                }),
                "Service name cannot be empty",
            ),
            // Empty paths object
            (
                json!({
                    "services": [
                        {
                            "name": "test",
                            "paths": {}
                        }
                    ]
                }),
                "has no paths",
            ),
            // Path not starting with /
            (
                json!({
                    "services": [
                        {
                            "name": "test",
                            "paths": {
                                "test": {}
                            }
                        }
                    ]
                }),
                "must start with '/'",
            ),
            // Path too long
            (
                json!({
                    "services": [
                        {
                            "name": "test",
                            "paths": {
                                format!("/{}", "x".repeat(1024)): {}
                            }
                        }
                    ]
                }),
                "too long",
            ),
            // Path with space
            (
                json!({
                    "services": [
                        {
                            "name": "test",
                            "paths": {
                                "/test path": {}
                            }
                        }
                    ]
                }),
                "contains space",
            ),
            // Duplicate paths
            (
                json!({
                    "services": [
                        {
                            "name": "service1",
                            "paths": {
                                "/test": {}
                            }
                        },
                        {
                            "name": "service2",
                            "paths": {
                                "/test": {}
                            }
                        }
                    ]
                }),
                "Duplicate or conflicting route",
            ),
        ];

        for (config, expected_error) in test_cases {
            let mut root_ctx = OpenapiEndpointRoot::new();
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

        let mut root_ctx = OpenapiEndpointRoot::new();
        root_ctx.configure(&config).unwrap();

        let http_ctx = OpenapiEndpointFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
            preserve_existing_headers: true,
            config_error: None,
        };

        // For exact path matches, the first service should win
        assert_eq!(
            http_ctx.get_path_template("/api/v1/shared"),
            Some((
                "/api/v1/shared".to_string(),
                Rc::new("service1".to_string())
            ))
        );

        // For parameterized paths, the match should work correctly
        assert_eq!(
            http_ctx.get_path_template("/api/v1/shared/123"),
            Some((
                "/api/v1/shared/{id}".to_string(),
                Rc::new("service2".to_string())
            ))
        );

        // Service-specific paths should go to the correct service
        assert_eq!(
            http_ctx.get_path_template("/api/v1/service1/specific"),
            Some((
                "/api/v1/service1/specific".to_string(),
                Rc::new("service1".to_string())
            ))
        );

        assert_eq!(
            http_ctx.get_path_template("/api/v1/service2/specific"),
            Some((
                "/api/v1/service2/specific".to_string(),
                Rc::new("service2".to_string())
            ))
        );
    }
}
