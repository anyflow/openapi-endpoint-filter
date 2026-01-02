use log::debug;
use matchit::Router;
use serde_json::Value;
use std::rc::Rc;

use crate::router::normalize_path;

#[derive(Clone, Debug)]
pub(crate) struct ServerSpec {
    pub(crate) host: Option<String>,
    pub(crate) base_path: String,
}

pub(crate) fn parse_servers(
    service: &Value,
) -> Result<Vec<ServerSpec>, Box<dyn std::error::Error>> {
    let servers_value = service.get("servers");
    if servers_value.is_none() {
        return Ok(vec![ServerSpec {
            host: None,
            base_path: String::new(),
        }]);
    }
    let servers = servers_value
        .and_then(Value::as_array)
        .ok_or("Invalid 'servers' in service configuration")?;
    if servers.is_empty() {
        return Err("Servers array cannot be empty".into());
    }

    let mut specs = Vec::new();
    for server in servers {
        let urls = expand_server_urls(server)?;
        for url in urls {
            specs.push(parse_server_url(&url)?);
        }
    }
    Ok(specs)
}

pub(crate) fn parse_methods(
    path: &str,
    path_config: &Value,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let object = path_config
        .as_object()
        .ok_or("Invalid path item configuration")?;
    if object.is_empty() {
        return Ok(Vec::new());
    }

    let mut methods = Vec::new();
    for key in object.keys() {
        let lower = key.to_ascii_lowercase();
        if is_http_method(&lower) {
            methods.push(lower);
        }
    }
    methods.sort();
    methods.dedup();
    if methods.is_empty() {
        debug!(
            "[oef] Path '{}' has no method entries; allowing all methods",
            path
        );
    }
    Ok(methods)
}

pub(crate) fn insert_route(
    router: &mut Router<(String, Rc<String>)>,
    path: &str,
    service_name: Rc<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("[oef] Inserting route: {} for service: {}", path, service_name);
    if let Err(e) = router.insert(path, (path.to_string(), service_name)) {
        return Err(format!("Duplicate or conflicting route '{}': {}", path, e).into());
    }
    Ok(())
}

pub(crate) fn strip_port(host: &str) -> &str {
    if host.starts_with('[') {
        if let Some(end) = host.find(']') {
            return &host[..end + 1];
        }
        return host;
    }
    if let Some(idx) = host.rfind(':') {
        let right = &host[idx + 1..];
        if !right.is_empty() && right.chars().all(|c| c.is_ascii_digit()) {
            return &host[..idx];
        }
    }
    host
}

fn expand_server_urls(server: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let url = server
        .get("url")
        .and_then(Value::as_str)
        .ok_or("Missing 'url' in server configuration")?;
    let variables = server.get("variables").and_then(Value::as_object);
    if variables.is_none() {
        return Ok(vec![url.to_string()]);
    }

    let mut expansions: Vec<(String, Vec<String>)> = Vec::new();
    for (name, spec) in variables.unwrap() {
        let spec = spec
            .as_object()
            .ok_or("Invalid 'variables' entry in server configuration")?;
        let values = if let Some(enum_values) = spec.get("enum").and_then(Value::as_array) {
            let mut vals = Vec::new();
            for value in enum_values {
                let val = value
                    .as_str()
                    .ok_or("Server variable enum values must be strings")?;
                vals.push(val.to_string());
            }
            if vals.is_empty() {
                return Err("Server variable enum cannot be empty".into());
            }
            vals
        } else if let Some(default) = spec.get("default").and_then(Value::as_str) {
            vec![default.to_string()]
        } else {
            return Err("Server variable must define 'enum' or 'default'".into());
        };
        expansions.push((name.to_string(), values));
    }

    let mut urls = vec![url.to_string()];
    for (name, values) in expansions {
        let placeholder = format!("{{{}}}", name);
        let mut next = Vec::new();
        for current in &urls {
            for value in &values {
                next.push(current.replace(&placeholder, value));
            }
        }
        if next.len() > 100 {
            return Err("Too many expanded server URLs (max 100)".into());
        }
        urls = next;
    }
    Ok(urls)
}

fn parse_server_url(url: &str) -> Result<ServerSpec, Box<dyn std::error::Error>> {
    let without_fragment = url.split('#').next().unwrap_or("");
    let without_query = without_fragment.split('?').next().unwrap_or("");
    let trimmed = without_query.trim();
    if trimmed.is_empty() {
        return Err("Server url cannot be empty".into());
    }

    let mut rest = trimmed;
    if let Some(idx) = trimmed.find("://") {
        rest = &trimmed[idx + 3..];
    }

    let (host, base_path) = if rest.starts_with('/') {
        (None, rest.to_string())
    } else {
        let mut parts = rest.splitn(2, '/');
        let host = parts.next().unwrap_or("");
        let base_path = parts.next().unwrap_or("");
        let host = if host.is_empty() {
            None
        } else {
            Some(strip_port(&host.to_ascii_lowercase()).to_string())
        };
        let base_path = if base_path.is_empty() {
            String::new()
        } else {
            format!("/{}", base_path)
        };
        (host, base_path)
    };

    let base_path = normalize_base_path(&base_path)?;
    Ok(ServerSpec {
        host,
        base_path,
    })
}

fn normalize_base_path(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    if path.is_empty() {
        return Ok(String::new());
    }
    let normalized = normalize_path(path);
    if normalized == "/" {
        return Ok(String::new());
    }
    Ok(normalized)
}

fn is_http_method(method: &str) -> bool {
    matches!(
        method,
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace"
    )
}
