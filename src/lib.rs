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
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action {
        match self.process_request_path() {
            Ok(_) => Action::Continue,
            Err(e) => {
                warn!("Error processing request path: {}", e);
                Action::Continue
            }
        }
    }
}

impl OpenapiPathHttpContext {
    fn process_request_path(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.get_http_request_header(":path")
            .ok_or("No path header found")?;

        match self.router.at(&path) {
            Ok(matched) => {
                self.set_http_request_header("x-openapi-path", Some(matched.value));
                Ok(())
            }
            Err(e) => {
                // 매칭되지 않는 경로는 warning으로 처리
                warn!("Path '{}' not found in OpenAPI spec: {}", path, e);
                Ok(())
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

    // 더 많은 테스트 케이스 추가 가능
}