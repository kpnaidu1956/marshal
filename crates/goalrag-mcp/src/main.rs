//! MCP server that proxies all 24 Goal-RAG LLM tools.
//!
//! Reads JSON-RPC requests from stdin, dispatches to the goal-rag HTTP API,
//! and writes JSON-RPC responses to stdout (STDIO transport).

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ============================================================================
// CLI
// ============================================================================

#[derive(Parser)]
#[command(name = "goalrag-mcp")]
#[command(about = "MCP server proxy for Goal-RAG LLM tools")]
#[command(version)]
struct Cli {
    /// Goal-RAG server base URL (or set GOALRAG_URL env var)
    #[arg(long)]
    url: Option<String>,

    /// Enable debug logging (to stderr)
    #[arg(short, long)]
    debug: bool,
}

// ============================================================================
// MCP Protocol Types
// ============================================================================

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct McpRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl McpResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(McpError { code, message: message.into(), data: None }),
        }
    }
}

const PARSE_ERROR: i32 = -32700;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

// ============================================================================
// MCP Tool type (for tools/list response)
// ============================================================================

#[derive(Debug, Serialize)]
struct McpTool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

// ============================================================================
// Goal-RAG API types (from HTTP responses)
// ============================================================================

#[derive(Debug, Deserialize)]
struct ToolDefinition {
    name: String,
    description: String,
    #[allow(dead_code)]
    category: String,
    parameters: Value,
}

#[derive(Debug, Deserialize)]
struct ToolResult {
    success: bool,
    data: Value,
    summary: String,
    row_count: usize,
    execution_ms: u64,
}

// ============================================================================
// Handler
// ============================================================================

struct Handler {
    client: reqwest::Client,
    base_url: String,
}

impl Handler {
    fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        Self { client, base_url }
    }

    async fn handle(&self, request: McpRequest) -> Option<McpResponse> {
        // Notifications (no id) don't get a response
        if request.id.is_none() {
            return None;
        }

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_tools_list(request.id).await,
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            _ => McpResponse::error(request.id, METHOD_NOT_FOUND, "Method not found"),
        };
        Some(response)
    }

    fn handle_initialize(&self, id: Option<Value>) -> McpResponse {
        McpResponse::success(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "goalrag-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    async fn handle_tools_list(&self, id: Option<Value>) -> McpResponse {
        let url = format!("{}/api/tools/manifest", self.base_url);

        let definitions: Vec<ToolDefinition> = match self.client.get(&url).send().await {
            Ok(resp) => match resp.json().await {
                Ok(defs) => defs,
                Err(e) => return McpResponse::error(id, INTERNAL_ERROR, format!("Failed to parse manifest: {}", e)),
            },
            Err(e) => return McpResponse::error(id, INTERNAL_ERROR, format!("Failed to fetch manifest: {}", e)),
        };

        let tools: Vec<McpTool> = definitions
            .into_iter()
            .map(|def| McpTool {
                name: def.name,
                description: def.description,
                input_schema: def.parameters,
            })
            .collect();

        McpResponse::success(id, json!({ "tools": tools }))
    }

    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> McpResponse {
        let params = match params {
            Some(p) => p,
            None => return McpResponse::error(id, INVALID_PARAMS, "Missing params"),
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(name) => name.to_string(),
            None => return McpResponse::error(id, INVALID_PARAMS, "Missing params.name"),
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let url = format!("{}/api/tools/execute", self.base_url);
        let body = json!({
            "tool": tool_name,
            "params": arguments,
        });

        let response = match self.client.post(&url).json(&body).send().await {
            Ok(resp) => resp,
            Err(e) => return McpResponse::error(id, INTERNAL_ERROR, format!("HTTP request failed: {}", e)),
        };

        let status = response.status();
        let response_text = match response.text().await {
            Ok(text) => text,
            Err(e) => return McpResponse::error(id, INTERNAL_ERROR, format!("Failed to read response: {}", e)),
        };

        if !status.is_success() {
            return McpResponse::error(
                id,
                INTERNAL_ERROR,
                format!("Tool execution failed (HTTP {}): {}", status.as_u16(), response_text),
            );
        }

        // Parse ToolResult
        match serde_json::from_str::<ToolResult>(&response_text) {
            Ok(result) => {
                let text = if result.success {
                    json!({
                        "data": result.data,
                        "summary": result.summary,
                        "row_count": result.row_count,
                        "execution_ms": result.execution_ms,
                    }).to_string()
                } else {
                    json!({
                        "error": true,
                        "summary": result.summary,
                    }).to_string()
                };

                McpResponse::success(id, json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": !result.success,
                }))
            }
            Err(_) => {
                // Response wasn't a ToolResult — return raw text
                McpResponse::success(id, json!({
                    "content": [{ "type": "text", "text": response_text }],
                }))
            }
        }
    }
}

// ============================================================================
// STDIO Transport
// ============================================================================

async fn run_stdio(handler: &Handler) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: McpRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let resp = McpResponse::error(None, PARSE_ERROR, e.to_string());
                let json = serde_json::to_string(&resp)?;
                stdout.write_all(json.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
                continue;
            }
        };

        if let Some(response) = handler.handle(request).await {
            let json = serde_json::to_string(&response)?;
            stdout.write_all(json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Logging goes to stderr (stdout is reserved for MCP JSON-RPC)
    if cli.debug {
        tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter("goalrag_mcp=debug").init();
    } else {
        tracing_subscriber::fmt().with_writer(std::io::stderr).with_env_filter("goalrag_mcp=warn").init();
    }

    let base_url = cli.url
        .or_else(|| std::env::var("GOALRAG_URL").ok())
        .unwrap_or_else(|| "http://localhost:8080".to_string());
    let base_url = base_url.trim_end_matches('/').to_string();
    tracing::debug!("Goal-RAG base URL: {}", base_url);

    let handler = Handler::new(base_url);
    run_stdio(&handler).await
}
