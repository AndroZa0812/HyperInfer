//! MCP Gateway: multiplexes multiple upstream MCP servers behind a single
//! gateway endpoint, with per-team tool allow/deny policies and secret
//! header injection.

use std::collections::HashMap;
use std::sync::Arc;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::warn;

use crate::mcp::McpToolDefinition;

// ── Upstream transport ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamTransport {
    StreamableHttp { url: String },
    Sse { url: String },
}

// ── Upstream server config ────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamMcpServer {
    /// Namespace prefix for tools (e.g. "github", "linear").
    pub name: String,
    pub transport: UpstreamTransport,
    /// Server-side injected auth headers. Supports `$ENV_VAR` resolution.
    /// Never serialized out to clients.
    #[serde(default, skip_serializing)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// ── Team policy ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpTeamPolicy {
    /// Empty means allow all (unless denied).
    #[serde(default)]
    pub allow_patterns: Vec<String>,
    /// Deny always takes precedence over allow.
    #[serde(default)]
    pub deny_patterns: Vec<String>,
    /// When true, tools/list returns only the compact meta-tools.
    #[serde(default)]
    pub catalog_mode: bool,
}

impl McpTeamPolicy {
    /// Check if a tool is allowed for this team.
    ///
    /// Deny patterns always take precedence. If allow_patterns is empty,
    /// all tools are allowed (unless denied). Invalid regexes fail closed
    /// for allow and fail safe for deny.
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // Check deny patterns first (always take precedence)
        for pattern in &self.deny_patterns {
            match Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(tool_name) {
                        return false;
                    }
                }
                Err(e) => {
                    warn!("Invalid deny regex '{}': {}", pattern, e);
                    // Fail safe for deny: don't block on bad regex
                }
            }
        }

        // If allow_patterns is empty, allow all
        if self.allow_patterns.is_empty() {
            return true;
        }

        // Check allow patterns
        for pattern in &self.allow_patterns {
            match Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(tool_name) {
                        return true;
                    }
                }
                Err(e) => {
                    warn!("Invalid allow regex '{}': {}", pattern, e);
                    // Fail closed for allow: don't match on bad regex
                }
            }
        }

        false
    }
}

// ── Gateway ───────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct McpGateway {
    upstreams: Arc<RwLock<HashMap<String, UpstreamMcpServer>>>,
    team_policies: Arc<RwLock<HashMap<String, McpTeamPolicy>>>,
    http: reqwest::Client,
}

impl McpGateway {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build MCP gateway HTTP client");
        Self {
            upstreams: Arc::new(RwLock::new(HashMap::new())),
            team_policies: Arc::new(RwLock::new(HashMap::new())),
            http,
        }
    }

    pub async fn register_upstream(&self, server: UpstreamMcpServer) {
        let mut upstreams = self.upstreams.write().await;
        upstreams.insert(server.name.clone(), server);
    }

    pub async fn remove_upstream(&self, name: &str) -> bool {
        let mut upstreams = self.upstreams.write().await;
        upstreams.remove(name).is_some()
    }

    pub async fn list_upstreams(&self) -> Vec<UpstreamMcpServer> {
        let upstreams = self.upstreams.read().await;
        upstreams.values().cloned().collect()
    }

    pub async fn set_team_policy(&self, team_id: impl Into<String>, policy: McpTeamPolicy) {
        let mut policies = self.team_policies.write().await;
        policies.insert(team_id.into(), policy);
    }

    pub async fn get_team_policy(&self, team_id: &str) -> McpTeamPolicy {
        let policies = self.team_policies.read().await;
        policies.get(team_id).cloned().unwrap_or_default()
    }

    /// Resolve `$ENV_VAR` references in header values.
    fn resolve_headers(&self, headers: &HashMap<String, String>) -> HashMap<String, String> {
        let mut resolved = HashMap::new();
        for (key, value) in headers {
            let resolved_value = if let Some(env_var) = value.strip_prefix("$") {
                std::env::var(env_var).unwrap_or_default()
            } else {
                value.clone()
            };
            resolved.insert(key.clone(), resolved_value);
        }
        resolved
    }

    /// List all tools from all enabled upstream servers.
    ///
    /// Each tool is namespaced as `{server_name}__{tool_name}`.
    /// Failing upstreams are logged and skipped.
    pub async fn list_upstream_tools(&self) -> Vec<McpToolDefinition> {
        let snapshot: Vec<(String, UpstreamMcpServer)> = {
            let upstreams = self.upstreams.read().await;
            upstreams
                .iter()
                .filter(|(_, s)| s.enabled)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let mut all_tools = Vec::new();

        for (server_name, server) in snapshot {
            let url = match &server.transport {
                UpstreamTransport::StreamableHttp { url } => url.clone(),
                UpstreamTransport::Sse { url } => {
                    warn!(
                        server = %server_name,
                        url = %url,
                        "SSE upstream transport handshake is not yet implemented; skipping upstream"
                    );
                    continue;
                }
            };

            let headers = self.resolve_headers(&server.headers);

            let request = self.http.post(&url).json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": "1",
                "method": "tools/list",
                "params": {}
            }));

            let request = headers
                .into_iter()
                .fold(request, |req, (k, v)| req.header(k, v));

            match request.send().await {
                Ok(response) => match response.json::<serde_json::Value>().await {
                    Ok(body) => {
                        if let Some(tools) = body
                            .get("result")
                            .and_then(|r| r.get("tools"))
                            .and_then(|t| t.as_array())
                        {
                            for tool in tools {
                                let tool_name = tool
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let namespaced = format!("{}__{}", server_name, tool_name);

                                let mut tool_def: McpToolDefinition =
                                    match serde_json::from_value(tool.clone()) {
                                        Ok(d) => d,
                                        Err(e) => {
                                            warn!(
                                                "Failed to parse tool from upstream {}: {}",
                                                server_name, e
                                            );
                                            continue;
                                        }
                                    };
                                tool_def.name = namespaced;
                                all_tools.push(tool_def);
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to parse tools/list response from upstream {}: {}",
                            server_name, e
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        "Failed to call tools/list on upstream {}: {}",
                        server_name, e
                    );
                }
            }
        }

        all_tools
    }

    /// Call a tool on an upstream server.
    ///
    /// `namespaced_name` should be in the form `{server_name}__{tool_name}`.
    pub async fn call_upstream_tool(
        &self,
        namespaced_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (server, tool_name) = {
            let upstreams = self.upstreams.read().await;
            // Match the longest registered upstream name prefix `<server>__<tool>`
            // so server names containing `__` resolve unambiguously.
            let matched = upstreams
                .iter()
                .filter_map(|(name, srv)| {
                    namespaced_name
                        .strip_prefix(name)
                        .and_then(|rest| rest.strip_prefix("__"))
                        .map(|tool| (name.len(), srv.clone(), tool.to_string()))
                })
                .max_by_key(|(len, _, _)| *len);

            match matched {
                Some((_, srv, tool)) if !tool.is_empty() => (srv, tool),
                _ => {
                    return Err(format!(
                        "Upstream server not found for tool: {}",
                        namespaced_name
                    ));
                }
            }
        };

        if !server.enabled {
            return Err(format!("Upstream server is disabled: {}", server.name));
        }

        let url = match &server.transport {
            UpstreamTransport::StreamableHttp { url } => url.clone(),
            UpstreamTransport::Sse { .. } => {
                return Err(format!(
                    "SSE upstream transport handshake is not yet supported for server '{}'; use streamable_http",
                    server.name
                ));
            }
        };

        let headers = self.resolve_headers(&server.headers);

        let request = self.http.post(&url).json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        }));

        let request = headers
            .into_iter()
            .fold(request, |req, (k, v)| req.header(k, v));

        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to call upstream: {}", e))?;

        let body = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = body.get("error") {
            return Err(format!("Upstream error: {}", error));
        }

        body.get("result")
            .cloned()
            .ok_or_else(|| "No result in upstream response".to_string())
    }

    /// Return the catalog meta-tools for use in catalog mode.
    pub fn catalog_meta_tools() -> Vec<McpToolDefinition> {
        vec![
            McpToolDefinition {
                name: "mcp_search_tools".to_string(),
                description: "Search for available MCP tools across all upstream servers."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query to filter tools"
                        }
                    },
                    "required": ["query"]
                }),
            },
            McpToolDefinition {
                name: "mcp_call_tool".to_string(),
                description: "Call a specific MCP tool by its namespaced name.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Namespaced tool name (e.g. github__list_repos)"
                        },
                        "arguments": {
                            "type": "object",
                            "description": "Arguments to pass to the tool"
                        }
                    },
                    "required": ["name", "arguments"]
                }),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_allow_all_when_empty() {
        let policy = McpTeamPolicy::default();
        assert!(policy.is_tool_allowed("any_tool"));
        assert!(policy.is_tool_allowed("github__list_repos"));
    }

    #[test]
    fn test_policy_deny_overrides_allow() {
        let policy = McpTeamPolicy {
            allow_patterns: vec![".*".to_string()],
            deny_patterns: vec!["github__delete.*".to_string()],
            ..Default::default()
        };
        assert!(policy.is_tool_allowed("github__list_repos"));
        assert!(!policy.is_tool_allowed("github__delete_repo"));
    }

    #[test]
    fn test_headers_not_serialized() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "secret-token".to_string());

        let server = UpstreamMcpServer {
            name: "test".to_string(),
            transport: UpstreamTransport::StreamableHttp {
                url: "http://example.com".to_string(),
            },
            headers,
            enabled: true,
        };

        let json = serde_json::to_string(&server).unwrap();
        assert!(!json.contains("secret-token"));
        assert!(!json.contains("headers"));
    }
}
