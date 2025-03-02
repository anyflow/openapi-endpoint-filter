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
    router: Rc<Router<(String, String)>>, // (path_template, service_name)
    cache: Option<Rc<RefCell<LruCache<String, (String, String)>>>>,
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
            .get("cache_size")
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
                new_router.insert(path, (path.clone(), service_name.to_string()))?;
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
    router: Rc<Router<(String, String)>>,
    cache: Option<Rc<RefCell<LruCache<String, (String, String)>>>>,
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
    fn get_openapi_path(&self, path: &str) -> Option<(String, String)> {
        let normalized_path = path.split('?').next().unwrap_or("").to_string();
        if let Some(cache) = &self.cache {
            let mut cache = cache.borrow_mut();
            if let Some((matched_path, service_name)) = cache.get(&normalized_path) {
                debug!(
                    "[opf] Cache hit for path: {}, value: ({}, {})",
                    normalized_path, matched_path, service_name
                );
                return Some((matched_path.clone(), service_name.clone()));
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
                        (matched_path.clone(), service_name.clone()),
                    );
                }
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

    const TEST_CONFIG: &str = r#"{
        "cache_size": 5,
        "services": [
            {
                "name": "dockebi",
                "paths": {
                    "/dockebi/v1/stuff": {},
                    "/dockebi/v1/stuff/{id_}": {},
                    "/dockebi/v1/stuff/{id_}/child/{child_id}/hello": {}
                }
            }
        ]
    }"#;

    #[test]
    fn test_path_and_service_matching() {
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
                Some(("/dockebi/v1/stuff".to_string(), "dockebi".to_string())),
            ),
            (
                "/dockebi/v1/stuff/123",
                Some(("/dockebi/v1/stuff/{id_}".to_string(), "dockebi".to_string())),
            ),
            (
                "/dockebi/v1/stuff/123/child/456/hello",
                Some((
                    "/dockebi/v1/stuff/{id_}/child/{child_id}/hello".to_string(),
                    "dockebi".to_string(),
                )),
            ),
            (
                "/dockebi/v1/stuff/123?key=value",
                Some(("/dockebi/v1/stuff/{id_}".to_string(), "dockebi".to_string())),
            ),
            ("/dockebi/v1/other", None),
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
    fn test_cache_behavior() {
        let mut root_ctx = OpenapiPathRoot::new();
        root_ctx
            .configure(&serde_json::from_str(TEST_CONFIG).unwrap())
            .unwrap();

        let http_ctx = OpenapiPathFilter {
            router: Rc::clone(&root_ctx.router),
            cache: root_ctx.cache.as_ref().map(Rc::clone),
        };

        let path1 = "/dockebi/v1/stuff/123";
        let result1 = http_ctx.get_openapi_path(path1);
        assert_eq!(
            result1,
            Some(("/dockebi/v1/stuff/{id_}".to_string(), "dockebi".to_string()))
        );

        let result2 = http_ctx.get_openapi_path(path1);
        assert_eq!(
            result2,
            Some(("/dockebi/v1/stuff/{id_}".to_string(), "dockebi".to_string()))
        );

        if let Some(cache) = &root_ctx.cache {
            assert_eq!(cache.borrow().len(), 1);
            assert_eq!(cache.borrow().cap().get(), 5);
        } else {
            panic!("Cache should be enabled");
        }
    }
}
