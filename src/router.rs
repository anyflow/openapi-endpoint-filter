use log::debug;
use matchit::Router;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct RouterSet {
    pub(crate) by_host: HashMap<Option<String>, Vec<RouteGroup>>,
}

impl RouterSet {
    pub(crate) fn new() -> Self {
        Self {
            by_host: HashMap::new(),
        }
    }

    pub(crate) fn match_route(
        &self,
        host: Option<&str>,
        method: &str,
        path: &str,
    ) -> Option<(String, Rc<String>)> {
        let normalized_path = normalize_path(path);
        let host_key = host.map(|h| h.to_ascii_lowercase());
        let mut groups = Vec::new();

        if let Some(host) = host_key.as_ref() {
            if let Some(host_groups) = self.by_host.get(&Some(host.clone())) {
                groups.extend(host_groups.iter());
            }
        }
        if let Some(wildcard_groups) = self.by_host.get(&None) {
            groups.extend(wildcard_groups.iter());
        }

        for group in groups {
            if let Some(stripped_path) = group.strip_base_path(&normalized_path) {
                if let Some(router) = group.methods.get(method) {
                    if let Some(result) = Self::match_router(router, &stripped_path, path) {
                        return Some(result);
                    }
                }
                if let Some(result) = Self::match_router(&group.any_method, &stripped_path, path) {
                    return Some(result);
                }
            }
        }

        debug!(
            "[oef] No match found for host: {:?}, method: {}, path: {}",
            host, method, normalized_path
        );
        None
    }

    fn match_router(
        router: &Router<(String, Rc<String>)>,
        stripped_path: &str,
        original_path: &str,
    ) -> Option<(String, Rc<String>)> {
        match router.at(stripped_path) {
            Ok(matched) => {
                let (matched_path, service_name) = matched.value.clone();
                debug!(
                    "[oef] {} matched with {}, {}",
                    original_path, service_name, matched_path
                );
                Some((matched_path, service_name))
            }
            Err(_) => None,
        }
    }
}

pub(crate) struct RouteGroup {
    pub(crate) base_path: String,
    pub(crate) any_method: Router<(String, Rc<String>)>,
    pub(crate) methods: HashMap<String, Router<(String, Rc<String>)>>,
}

impl RouteGroup {
    pub(crate) fn new(base_path: String) -> Self {
        Self {
            base_path,
            any_method: Router::new(),
            methods: HashMap::new(),
        }
    }

    pub(crate) fn strip_base_path(&self, path: &str) -> Option<String> {
        if self.base_path.is_empty() {
            return Some(path.to_string());
        }
        if path == self.base_path {
            return Some("/".to_string());
        }
        if path.starts_with(&self.base_path) {
            let remainder = &path[self.base_path.len()..];
            if remainder.starts_with('/') {
                return Some(remainder.to_string());
            }
        }
        None
    }
}

pub(crate) fn normalize_path(path: &str) -> String {
    let without_query = path.split('?').next().unwrap_or("");
    let without_fragment = without_query.split('#').next().unwrap_or("");

    let segments: Vec<&str> = without_fragment
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        return "/".to_string();
    }

    format!("/{}", segments.join("/"))
}
