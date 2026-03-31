#![deny(warnings)]

use axum::{
    Router,
    extract::{State, ws::WebSocketUpgrade},
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Stdin, Stdout};
use tokio::net::TcpListener;

use crate::error::{Result, TaskMcpError, TransportError};
use crate::server::McpServer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StdioFraming {
    Auto,
    Newline,
    ContentLength,
}

fn trim_crlf(s: &str) -> &str {
    s.trim_end_matches(&['\r', '\n'][..])
}

fn parse_content_length_header(line: &str) -> Option<usize> {
    let line = trim_crlf(line).trim();
    let (name, value) = line.split_once(':')?;
    if !name.trim().eq_ignore_ascii_case("content-length") {
        return None;
    }
    value.trim().parse::<usize>().ok()
}

pub struct StdioTransportHandler {
    stdin: BufReader<Stdin>,
    stdout: Stdout,
    framing: StdioFraming,
}

impl Default for StdioTransportHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioTransportHandler {
    pub fn new() -> Self {
        Self {
            stdin: BufReader::new(io::stdin()),
            stdout: io::stdout(),
            framing: StdioFraming::Auto,
        }
    }

    pub async fn read_message(&mut self) -> Result<String> {
        match self.framing {
            StdioFraming::Auto => self.read_message_auto().await,
            StdioFraming::Newline => self.read_message_newline().await,
            StdioFraming::ContentLength => self.read_message_content_length().await,
        }
    }

    pub async fn write_message(&mut self, message: &str) -> Result<()> {
        match self.framing {
            StdioFraming::ContentLength => self.write_message_content_length(message).await,
            StdioFraming::Auto | StdioFraming::Newline => self.write_message_newline(message).await,
        }
    }

    async fn read_message_auto(&mut self) -> Result<String> {
        loop {
            let mut line = String::new();
            let n = self
                .stdin
                .read_line(&mut line)
                .await
                .map_err(TransportError::Io)?;
            if n == 0 {
                return Err(TransportError::ConnectionClosed.into());
            }

            let trimmed = trim_crlf(&line);
            if trimmed.trim().is_empty() {
                continue;
            }

            if parse_content_length_header(trimmed).is_some() {
                self.framing = StdioFraming::ContentLength;
                return self
                    .read_message_content_length_with_first_line(trimmed)
                    .await;
            }

            self.framing = StdioFraming::Newline;
            return Ok(trimmed.to_string());
        }
    }

    async fn read_message_newline(&mut self) -> Result<String> {
        let mut line = String::new();
        let n = self
            .stdin
            .read_line(&mut line)
            .await
            .map_err(TransportError::Io)?;
        if n == 0 {
            return Err(TransportError::ConnectionClosed.into());
        }
        Ok(trim_crlf(&line).to_string())
    }

    async fn read_message_content_length(&mut self) -> Result<String> {
        let mut first_line = String::new();
        let n = self
            .stdin
            .read_line(&mut first_line)
            .await
            .map_err(TransportError::Io)?;
        if n == 0 {
            return Err(TransportError::ConnectionClosed.into());
        }
        self.read_message_content_length_with_first_line(trim_crlf(&first_line))
            .await
    }

    async fn read_message_content_length_with_first_line(
        &mut self,
        first_line: &str,
    ) -> Result<String> {
        const MAX_CONTENT_LENGTH: usize = 10 * 1024 * 1024; // 10 MiB

        let content_length = parse_content_length_header(first_line).ok_or_else(|| {
            TransportError::InvalidMessage(format!(
                "expected Content-Length header, got: {first_line}"
            ))
        })?;

        if content_length > MAX_CONTENT_LENGTH {
            return Err(TransportError::InvalidMessage(format!(
                "Content-Length {content_length} exceeds maximum ({MAX_CONTENT_LENGTH})"
            ))
            .into());
        }

        loop {
            let mut header_line = String::new();
            let n = self
                .stdin
                .read_line(&mut header_line)
                .await
                .map_err(TransportError::Io)?;
            if n == 0 {
                return Err(TransportError::ConnectionClosed.into());
            }
            if trim_crlf(&header_line).is_empty() {
                break;
            }
        }

        let mut body = vec![0_u8; content_length];
        self.stdin
            .read_exact(&mut body)
            .await
            .map_err(TransportError::Io)?;

        String::from_utf8(body)
            .map_err(|e| TransportError::InvalidMessage(format!("invalid UTF-8 payload: {e}")))
            .map_err(Into::into)
    }

    async fn write_message_newline(&mut self, message: &str) -> Result<()> {
        self.stdout
            .write_all(message.as_bytes())
            .await
            .map_err(TransportError::Io)?;
        self.stdout
            .write_all(b"\n")
            .await
            .map_err(TransportError::Io)?;
        self.stdout.flush().await.map_err(TransportError::Io)?;
        Ok(())
    }

    async fn write_message_content_length(&mut self, message: &str) -> Result<()> {
        let bytes = message.as_bytes();
        let header = format!("Content-Length: {}\r\n\r\n", bytes.len());
        self.stdout
            .write_all(header.as_bytes())
            .await
            .map_err(TransportError::Io)?;
        self.stdout
            .write_all(bytes)
            .await
            .map_err(TransportError::Io)?;
        self.stdout.flush().await.map_err(TransportError::Io)?;
        Ok(())
    }
}

pub async fn run_stdio_server(server: McpServer) -> Result<()> {
    let server = Arc::new(server);
    let mut transport = StdioTransportHandler::new();

    loop {
        let message_str = match transport.read_message().await {
            Ok(message) => message,
            Err(error) => {
                eprintln!("Error reading message: {error}");
                break;
            }
        };

        if message_str.is_empty() {
            continue;
        }

        let message: Value = match serde_json::from_str(&message_str) {
            Ok(message) => message,
            Err(error) => {
                let response =
                    jsonrpc_error_response(None, -32700, "Parse error", Some(error.to_string()));
                let serialized = serde_json::to_string(&response)?;
                transport.write_message(&serialized).await?;
                continue;
            }
        };

        if let Some(response) = handle_jsonrpc_message(server.clone(), message).await {
            let serialized = serde_json::to_string(&response)?;
            transport.write_message(&serialized).await?;
        }
    }

    Ok(())
}

pub async fn run_websocket_server(server: McpServer, host: &str, port: u16) -> Result<()> {
    let server = Arc::new(server);
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .with_state(server);

    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).await?;
    eprintln!("WebSocket server listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn websocket_handler(ws: WebSocketUpgrade, State(server): State<Arc<McpServer>>) -> Response {
    ws.on_upgrade(move |socket| handle_websocket_connection(socket, server))
}

async fn handle_websocket_connection(socket: axum::extract::ws::WebSocket, server: Arc<McpServer>) {
    use axum::extract::ws::Message;

    let (mut sender, mut receiver) = socket.split();

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let message: Value = match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(error) => {
                        let response = jsonrpc_error_response(
                            None,
                            -32700,
                            "Parse error",
                            Some(error.to_string()),
                        );
                        if let Ok(serialized) = serde_json::to_string(&response) {
                            let _ = sender.send(Message::Text(serialized.into())).await;
                        }
                        continue;
                    }
                };

                if let Some(response) = handle_jsonrpc_message(server.clone(), message).await
                    && let Ok(serialized) = serde_json::to_string(&response)
                {
                    let _ = sender.send(Message::Text(serialized.into())).await;
                }
            }
            Ok(Message::Close(_)) => break,
            Err(error) => {
                eprintln!("WebSocket error: {error}");
                break;
            }
            _ => {}
        }
    }
}

async fn handle_jsonrpc_message(server: Arc<McpServer>, message: Value) -> Option<Value> {
    if let Some(version) = message.get("jsonrpc").and_then(Value::as_str)
        && version != "2.0"
    {
        let id = message.get("id").cloned();
        return Some(jsonrpc_error_response(
            id,
            -32600,
            "Invalid JSON-RPC version",
            Some(version.to_string()),
        ));
    }

    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str);
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let is_notification = id.is_none();

    let result = match method {
        Some("initialize") => {
            let protocol_version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2024-11-05");
            let client_capabilities = params.get("capabilities").unwrap_or(&Value::Null);
            server
                .handle_initialize(protocol_version, client_capabilities)
                .await
        }
        Some("initialized") | Some("notifications/initialized") => {
            match server.handle_initialized().await {
                Ok(_) => Ok(Value::Null),
                Err(error) => Err(error),
            }
        }
        Some("tools/list") => {
            if !server.is_initialized().await {
                return Some(jsonrpc_error_response(
                    id,
                    -32000,
                    "Server not initialized. Call 'initialize' first.",
                    None,
                ));
            }
            Ok(json!({"tools": server.list_tools()}))
        }
        Some("tools/call") => {
            if !server.is_initialized().await {
                return Some(jsonrpc_error_response(
                    id,
                    -32000,
                    "Server not initialized. Call 'initialize' first.",
                    None,
                ));
            }

            let tool_name = match params.get("name").and_then(Value::as_str) {
                Some(name) => name,
                None => {
                    return Some(jsonrpc_error_response(
                        id,
                        -32602,
                        "Invalid params: missing tool name",
                        None,
                    ));
                }
            };
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match server.call_tool(tool_name, args).await {
                Ok(tool_result) => Ok(json!({
                    "content": [{"type": "text", "text": tool_result.to_string()}],
                    "structuredContent": tool_result,
                    "isError": false
                })),
                Err(error) => Ok(json!({
                    "content": [{"type": "text", "text": error.to_string()}],
                    "isError": true
                })),
            }
        }
        Some("shutdown") => Ok(Value::Null),
        _ => Err(TaskMcpError::InvalidArgument(
            "Method not found".to_string(),
        )),
    };

    if is_notification {
        return None;
    }

    let response = match result {
        Ok(payload) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": payload,
        }),
        Err(error) => jsonrpc_error_response(id, -32603, "Internal error", Some(error.to_string())),
    };

    Some(response)
}

fn jsonrpc_error_response(
    id: Option<Value>,
    code: i64,
    message: &str,
    data: Option<String>,
) -> Value {
    let mut error_obj = json!({
        "code": code,
        "message": message,
    });

    if let Some(data) = data
        && let Some(map) = error_obj.as_object_mut()
    {
        map.insert("data".to_string(), Value::String(data));
    }

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": error_obj,
    })
}
