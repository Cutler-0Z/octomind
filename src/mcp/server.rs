// Copyright 2025 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// External MCP server provider

use super::process;
use super::{McpFunction, McpToolCall, McpToolResult};
use crate::config::{Config, McpConnectionType, McpServerConfig};
use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// Global cache for server function definitions to avoid repeated JSON-RPC calls
// Functions are cached until server restarts (no TTL needed)
lazy_static::lazy_static! {
	static ref FUNCTION_CACHE: Arc<RwLock<HashMap<String, Vec<McpFunction>>>> =
		Arc::new(RwLock::new(HashMap::new()));
}

// Shared JSON-RPC message builders for MCP protocol
pub fn create_tools_list_request() -> Value {
	json!({
		"jsonrpc": "2.0",
		"id": 1,
		"method": "tools/list",
		"params": {}
	})
}

pub fn create_initialize_request() -> Value {
	json!({
		"jsonrpc": "2.0",
		"id": 1,
		"method": "initialize",
		"params": {
			"protocolVersion": "2024-11-05",
			"capabilities": {},
			"clientInfo": {
				"name": "octomind-health-check",
				"version": "1.0.0"
			}
		}
	})
}

fn create_tools_call_request(tool_name: &str, parameters: &Value) -> Value {
	json!({
		"jsonrpc": "2.0",
		"id": 1,
		"method": "tools/call",
		"params": {
			"name": tool_name,
			"arguments": parameters
		}
	})
}

// Shared function to parse tools from JSON-RPC response
fn parse_tools_from_jsonrpc_response(
	response: &Value,
	server: &McpServerConfig,
) -> Result<Vec<McpFunction>> {
	let mut functions = Vec::new();

	// Check for JSON-RPC error
	if let Some(error) = response.get("error") {
		return Err(anyhow::anyhow!("JSON-RPC error from MCP server: {}", error));
	}

	// Extract tools from result.tools
	if let Some(result) = response.get("result") {
		if let Some(tools) = result.get("tools").and_then(|t| t.as_array()) {
			for tool in tools {
				if let (Some(name), Some(description)) = (
					tool.get("name").and_then(|n| n.as_str()),
					tool.get("description").and_then(|d| d.as_str()),
				) {
					// Check if this tool is enabled
					if server.tools().is_empty()
						|| crate::mcp::is_tool_allowed_by_patterns(name, server.tools())
					{
						// Get the parameters from the inputSchema field if available
						let parameters = tool.get("inputSchema").cloned().unwrap_or(json!({}));

						functions.push(McpFunction {
							name: name.to_string(),
							description: description.to_string(),
							parameters,
						});
					}
				}
			}
		}
	} else {
		return Err(anyhow::anyhow!(
			"Invalid JSON-RPC response: missing 'result' field"
		));
	}

	Ok(functions)
}

// Get server function definitions (will start server if needed)
pub async fn get_server_functions(server: &McpServerConfig) -> Result<Vec<McpFunction>> {
	// Note: enabled check is now handled at the role level via server_refs
	// All servers in the registry are considered available

	// Handle different server connection types
	match server.connection_type() {
		McpConnectionType::Http => {
			// Handle local vs remote servers
			let server_url = get_server_base_url(server).await?;

			// Create a client
			let client = Client::new();

			// Prepare headers
			let mut headers = HeaderMap::new();
			headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

			// Add auth token if present
			if let Some(token) = server.auth_token() {
				headers.insert(
					AUTHORIZATION,
					HeaderValue::from_str(&format!("Bearer {}", token))?,
				);
			}

			// MCP uses JSON-RPC over HTTP with POST requests
			let schema_url = server_url; // Use base URL for JSON-RPC

			// Use shared JSON-RPC request builder
			let jsonrpc_request = create_tools_list_request();

			// Debug output
			crate::log_debug!(
				"Making JSON-RPC tools/list request to HTTP server '{}' at URL: {}",
				server.name(),
				schema_url
			);

			// Make JSON-RPC POST request to get schema
			let response = client
				.post(&schema_url)
				.headers(headers.clone())
				.json(&jsonrpc_request)
				.send()
				.await?;

			// Check if request was successful
			if !response.status().is_success() {
				return Err(anyhow::anyhow!(
					"Failed to get schema from MCP server: {}",
					response.status()
				));
			}

			// Parse JSON-RPC response
			let jsonrpc_response: Value = response.json().await?;

			crate::log_debug!(
				"JSON-RPC response from server '{}': {}",
				server.name(),
				serde_json::to_string_pretty(&jsonrpc_response)
					.unwrap_or_else(|_| jsonrpc_response.to_string())
			);

			// Use shared parser
			let functions = parse_tools_from_jsonrpc_response(&jsonrpc_response, server)?;

			Ok(functions)
		}
		McpConnectionType::Stdin => {
			// For stdin-based servers, ensure the server is running and get functions
			process::ensure_server_running(server).await?;
			process::get_stdin_server_functions(server).await
		}
		McpConnectionType::Builtin => {
			// Built-in servers don't need external processes
			Err(anyhow::anyhow!(
				"Built-in servers should not use get_server_functions"
			))
		}
	}
}

// Get server function definitions WITHOUT making JSON-RPC calls (optimized for system prompt generation)
pub async fn get_server_functions_cached(server: &McpServerConfig) -> Result<Vec<McpFunction>> {
	let server_id = server.name();

	// First, check if we have cached functions
	{
		let cache = FUNCTION_CACHE.read().unwrap();
		if let Some(cached_functions) = cache.get(server_id) {
			return Ok(cached_functions.clone());
		}
	}

	// Check if server is currently running
	let is_running = is_server_running_for_cache_check(server);

	if is_running {
		// Server is running - get fresh functions and cache them
		crate::log_debug!(
			"Server '{}' is running - fetching and caching function definitions",
			server_id
		);

		match get_server_functions(server).await {
			Ok(functions) => {
				// Cache the functions (no expiration - only cleared on server restart)
				{
					let mut cache = FUNCTION_CACHE.write().unwrap();
					cache.insert(server_id.to_string(), functions.clone());
				}
				crate::log_debug!(
					"Cached {} functions for server '{}'",
					functions.len(),
					server_id
				);
				Ok(functions)
			}
			Err(e) => {
				crate::log_error!(
					"Failed to get functions from running server '{}': {}",
					server_id,
					e
				);
				// Fall back to configured tools
				get_fallback_functions(server)
			}
		}
	} else {
		// Server is not running - return configured tools or empty list
		crate::log_debug!(
			"Server '{}' is not running - using fallback function definitions",
			server_id
		);
		get_fallback_functions(server)
	}
}

// Helper function to get fallback functions when server is not running
fn get_fallback_functions(server: &McpServerConfig) -> Result<Vec<McpFunction>> {
	if !server.tools().is_empty() {
		// For remote HTTP servers, don't show "server not started" since they're external
		let description_suffix = if server.connection_type()
			== crate::config::McpConnectionType::Http
			&& server.url().is_some()
			&& server.command().is_none()
		{
			"(remote server)"
		} else {
			"(server not started)"
		};

		// Return lightweight function entries based on configuration
		Ok(server
			.tools()
			.iter()
			.map(|tool_name| McpFunction {
				name: tool_name.clone(),
				description: format!(
					"External tool '{}' from server '{}' {}",
					tool_name,
					server.name(),
					description_suffix
				),
				parameters: serde_json::json!({}),
			})
			.collect())
	} else {
		// No specific tools configured and server not running
		Ok(vec![])
	}
}

// Optimized server running check that doesn't hold locks for long
// This function now requires server config to properly handle remote HTTP servers
fn is_server_running_for_cache_check(server: &McpServerConfig) -> bool {
	// For remote HTTP servers (have URL but no command), consider them always available
	if server.connection_type() == McpConnectionType::Http
		&& server.url().is_some()
		&& server.command().is_none()
	{
		crate::log_debug!(
			"Remote HTTP server '{}' is considered always available",
			server.name()
		);
		return true;
	}

	// For local servers (have command) or stdin servers, check the process registry
	let processes = process::SERVER_PROCESSES.read().unwrap();
	if let Some(process_arc) = processes.get(server.name()) {
		// Try to get a quick lock - if we can't, assume it's busy and running
		if let Ok(mut process) = process_arc.try_lock() {
			match &mut *process {
				process::ServerProcess::Http(child) => child
					.try_wait()
					.map(|status| status.is_none())
					.unwrap_or(false),
				process::ServerProcess::Stdin {
					child, is_shutdown, ..
				} => {
					let process_alive = child
						.try_wait()
						.map(|status| status.is_none())
						.unwrap_or(false);
					let not_marked_shutdown =
						!is_shutdown.load(std::sync::atomic::Ordering::SeqCst);
					process_alive && not_marked_shutdown
				}
			}
		} else {
			// If we can't get the lock, assume the server is busy and running
			true
		}
	} else {
		false
	}
}

// Clear cached functions for a specific server (called when server restarts)
pub fn clear_function_cache_for_server(server_name: &str) {
	let mut cache = FUNCTION_CACHE.write().unwrap();
	if cache.remove(server_name).is_some() {
		crate::log_debug!(
			"Cleared function cache for server '{}' due to restart",
			server_name
		);
	}
}

// Clear all cached functions (useful for cleanup)
pub fn clear_all_function_cache() {
	let mut cache = FUNCTION_CACHE.write().unwrap();
	let count = cache.len();
	cache.clear();
	if count > 0 {
		crate::log_debug!("Cleared function cache for {} servers", count);
	}
}

// Check if a server is already running with enhanced health checking
// Takes server config to properly handle internal vs external servers
pub fn is_server_already_running_with_config(server: &crate::config::McpServerConfig) -> bool {
	match server.connection_type() {
		McpConnectionType::Builtin => {
			// Internal servers are always considered running since they're built-in
			{
				let mut restart_info_guard = process::SERVER_RESTART_INFO.write().unwrap();
				let info = restart_info_guard
					.entry(server.name().to_string())
					.or_default();
				info.health_status = process::ServerHealth::Running;
				info.last_health_check = Some(std::time::SystemTime::now());
			}
			true
		}
		McpConnectionType::Http | McpConnectionType::Stdin => {
			// For remote HTTP servers (have URL but no command), consider them always available
			if server.connection_type() == McpConnectionType::Http
				&& server.url().is_some()
				&& server.command().is_none()
			{
				crate::log_debug!(
					"Remote HTTP server '{}' is considered always available",
					server.name()
				);
				// Update health status for remote servers
				{
					let mut restart_info_guard = process::SERVER_RESTART_INFO.write().unwrap();
					let info = restart_info_guard
						.entry(server.name().to_string())
						.or_default();
					info.health_status = process::ServerHealth::Running;
					info.last_health_check = Some(std::time::SystemTime::now());
				}
				return true;
			}

			// External servers with local processes - check the process registry
			let is_process_running = {
				let processes = process::SERVER_PROCESSES.read().unwrap();
				if let Some(process_arc) = processes.get(server.name()) {
					let mut process = process_arc.lock().unwrap();
					match &mut *process {
						process::ServerProcess::Http(child) => child
							.try_wait()
							.map(|status| status.is_none())
							.unwrap_or(false),
						process::ServerProcess::Stdin {
							child, is_shutdown, ..
						} => {
							let process_alive = child
								.try_wait()
								.map(|status| status.is_none())
								.unwrap_or(false);
							let not_marked_shutdown =
								!is_shutdown.load(std::sync::atomic::Ordering::SeqCst);
							process_alive && not_marked_shutdown
						}
					}
				} else {
					false
				}
			};

			// Update health status based on actual process state
			let health_status = if is_process_running {
				process::ServerHealth::Running
			} else {
				process::ServerHealth::Dead
			};

			// Update restart tracking
			{
				let mut restart_info_guard = process::SERVER_RESTART_INFO.write().unwrap();
				let info = restart_info_guard
					.entry(server.name().to_string())
					.or_default();
				info.health_status = health_status;
				info.last_health_check = Some(std::time::SystemTime::now());
			}

			is_process_running
		}
	}
}

// Legacy function for backward compatibility - tries to guess server type
pub fn is_server_already_running(server_name: &str) -> bool {
	// For internal servers, we need to determine their type first
	// Internal servers (Developer/Filesystem) are always "running" since they're built-in

	// Check if this is an internal server by looking for it in a typical config
	// This is a bit of a hack, but we need to distinguish internal vs external servers
	if server_name == "developer" || server_name == "filesystem" {
		// Internal servers are always considered running
		let mut restart_info_guard = process::SERVER_RESTART_INFO.write().unwrap();
		let info = restart_info_guard
			.entry(server_name.to_string())
			.or_default();
		info.health_status = process::ServerHealth::Running;
		info.last_health_check = Some(std::time::SystemTime::now());
		return true;
	}

	// For external servers, check the process registry
	let is_process_running = {
		let processes = process::SERVER_PROCESSES.read().unwrap();
		if let Some(process_arc) = processes.get(server_name) {
			let mut process = process_arc.lock().unwrap();
			match &mut *process {
				process::ServerProcess::Http(child) => child
					.try_wait()
					.map(|status| status.is_none())
					.unwrap_or(false),
				process::ServerProcess::Stdin {
					child, is_shutdown, ..
				} => {
					let process_alive = child
						.try_wait()
						.map(|status| status.is_none())
						.unwrap_or(false);
					let not_marked_shutdown =
						!is_shutdown.load(std::sync::atomic::Ordering::SeqCst);
					process_alive && not_marked_shutdown
				}
			}
		} else {
			false
		}
	};

	// Update health status based on actual process state
	let health_status = if is_process_running {
		process::ServerHealth::Running
	} else {
		process::ServerHealth::Dead
	};

	// Update restart tracking
	{
		let mut restart_info_guard = process::SERVER_RESTART_INFO.write().unwrap();
		let info = restart_info_guard
			.entry(server_name.to_string())
			.or_default();
		info.health_status = health_status;
		info.last_health_check = Some(std::time::SystemTime::now());
	}

	is_process_running
}

// Execute tool call on MCP server (either local or remote)
pub async fn execute_tool_call(
	call: &McpToolCall,
	server: &McpServerConfig,
	cancellation_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<McpToolResult> {
	use std::sync::atomic::Ordering;

	// Check for cancellation before starting
	if let Some(ref token) = cancellation_token {
		if token.load(Ordering::SeqCst) {
			return Err(anyhow::anyhow!("External tool execution cancelled"));
		}
	}

	// Check server health before attempting execution (but don't restart)
	let server_health = process::get_server_health(server.name());
	match server_health {
		process::ServerHealth::Failed => {
			return Err(anyhow::anyhow!(
				"Server '{}' is in failed state. Cannot execute tool '{}'. Server will not be restarted automatically.",
				server.name(),
				call.tool_name
			));
		}
		process::ServerHealth::Restarting => {
			return Err(anyhow::anyhow!(
				"Server '{}' is currently starting. Please try again in a moment.",
				server.name()
			));
		}
		process::ServerHealth::Dead => {
			return Err(anyhow::anyhow!(
				"Server '{}' is not running. Cannot execute tool '{}'. Server will not be restarted automatically.",
				server.name(),
				call.tool_name
			));
		}
		process::ServerHealth::Running => {
			// Server is running, proceed with execution
		}
	}

	// Execute the tool call directly (no restart logic)
	execute_tool_call_internal(call, server, cancellation_token).await
}

// Internal function to execute tool call without restart logic
async fn execute_tool_call_internal(
	call: &McpToolCall,
	server: &McpServerConfig,
	cancellation_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<McpToolResult> {
	use std::sync::atomic::Ordering;

	// Check for cancellation before starting
	if let Some(ref token) = cancellation_token {
		if token.load(Ordering::SeqCst) {
			return Err(anyhow::anyhow!("External tool execution cancelled"));
		}
	}

	// Note: enabled check is now handled at the role level via server_refs
	// All servers in the registry are considered available for execution

	// Extract tool name and parameters
	let tool_name = &call.tool_name;
	let parameters = &call.parameters;

	// Tool execution display is now handled in response.rs to avoid duplication

	// Handle different server connection types
	match server.connection_type() {
		McpConnectionType::Http => {
			// Check for cancellation before HTTP request
			if let Some(ref token) = cancellation_token {
				if token.load(Ordering::SeqCst) {
					return Err(anyhow::anyhow!("External tool execution cancelled"));
				}
			}

			// Handle local vs remote servers for HTTP mode
			let server_url = get_server_base_url(server).await?;

			// Create a client with configured timeout
			let client = Client::builder()
				.timeout(std::time::Duration::from_secs(server.timeout_seconds()))
				.build()
				.unwrap_or_else(|_| Client::new());

			// Prepare headers
			let mut headers = HeaderMap::new();
			headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

			// Add auth token if present
			if let Some(token) = server.auth_token() {
				headers.insert(
					AUTHORIZATION,
					HeaderValue::from_str(&format!("Bearer {}", token))?,
				);
			}

			// Use base URL for JSON-RPC tool execution
			let execute_url = server_url;

			// Use shared JSON-RPC request builder
			let request_body = create_tools_call_request(tool_name, parameters);

			// Check for cancellation one more time before sending request
			if let Some(ref token) = cancellation_token {
				if token.load(Ordering::SeqCst) {
					return Err(anyhow::anyhow!("External tool execution cancelled"));
				}
			}

			// Make request to execute tool
			let response = client
				.post(&execute_url)
				.headers(headers)
				.json(&request_body)
				.send()
				.await?;

			// Check if request was successful
			if !response.status().is_success() {
				// Save the status before consuming the response with text()
				let status = response.status();
				let error_text = response.text().await?;
				return Err(anyhow::anyhow!(
					"Failed to execute tool on MCP server: {}, {}",
					status,
					error_text
				));
			}

			// Parse JSON-RPC response
			let result: Value = response.json().await?;

			// Extract result or error from the JSON-RPC response
			let output = if let Some(error) = result.get("error") {
				json!({
					"error": true,
					"success": false,
					"message": error.get("message").and_then(|m| m.as_str()).unwrap_or("Server error")
				})
			} else {
				result.get("result").cloned().unwrap_or(json!("No result"))
			};

			// Create MCP-compliant tool result
			let tool_result = McpToolResult::success(
				tool_name.clone(),
				call.tool_id.clone(),
				serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string()),
			);

			Ok(tool_result)
		}
		McpConnectionType::Stdin => {
			// For stdin-based servers, use the stdin communication channel with cancellation support
			process::execute_stdin_tool_call(call, server, cancellation_token).await
		}
		McpConnectionType::Builtin => {
			// Built-in servers should not use this function
			Err(anyhow::anyhow!(
				"Built-in servers should not use execute_tool_call"
			))
		}
	}
}

// Get the base URL for a server, starting it if necessary for local servers
async fn get_server_base_url(server: &McpServerConfig) -> Result<String> {
	match server.connection_type() {
		McpConnectionType::Http => {
			// First check if this is a remote server with a URL (should not be started)
			if let Some(url) = server.url() {
				// This is a remote server with a URL - return it directly
				crate::log_debug!(
					"Using remote HTTP server '{}' at URL: {}",
					server.name(),
					url
				);
				let base_url = url.trim_end_matches("/").to_string();
				crate::log_debug!(
					"Processed base URL for server '{}': {}",
					server.name(),
					base_url
				);
				Ok(base_url)
			} else if server.command().is_some() {
				// This is a local server, ensure it's running
				crate::log_debug!(
					"Starting local HTTP server '{}' with command: {:?}",
					server.name(),
					server.command()
				);
				process::ensure_server_running(server).await
			} else {
				// Neither remote nor local configuration
				Err(anyhow::anyhow!("Invalid server configuration: neither URL nor command specified for server '{}'", server.name()))
			}
		}
		McpConnectionType::Stdin => {
			// For stdin-based servers, return a pseudo-URL
			if server.command().is_some() {
				// Ensure the stdin server is running
				process::ensure_server_running(server).await
			} else {
				Err(anyhow::anyhow!("Invalid server configuration: command not specified for stdin-based server '{}'", server.name()))
			}
		}
		McpConnectionType::Builtin => {
			// Built-in servers don't have URLs
			Err(anyhow::anyhow!("Built-in servers don't have URLs"))
		}
	}
}

// Get all available functions from all configured servers
pub async fn get_all_server_functions(
	config: &Config,
) -> Result<HashMap<String, (McpFunction, McpServerConfig)>> {
	let mut functions = HashMap::new();

	// Only proceed if MCP has any servers configured
	if config.mcp.servers.is_empty() {
		return Ok(functions);
	}

	// Get available servers from merged config (which should already be filtered by server_refs)
	let servers: Vec<crate::config::McpServerConfig> = config.mcp.servers.to_vec();

	// Check each server
	for server in &servers {
		let server_functions = get_server_functions(server).await?;

		for func in server_functions {
			functions.insert(func.name.clone(), (func, server.clone()));
		}
	}

	Ok(functions)
}

// Clean up any running server processes when the program exits
pub fn cleanup_servers() -> Result<()> {
	// Stop the health monitor first
	crate::mcp::health_monitor::stop_health_monitor();

	// Then stop all server processes
	process::stop_all_servers()
}

// Get server health status for monitoring
pub fn get_server_health_status(server_name: &str) -> process::ServerHealth {
	process::get_server_health(server_name)
}

// Get detailed server restart information
pub fn get_server_restart_info(server_name: &str) -> process::ServerRestartInfo {
	process::get_server_restart_info(server_name)
}

// Reset server failure state (useful for manual recovery)
pub fn reset_server_failure_state(server_name: &str) -> Result<()> {
	process::reset_server_failure_state(server_name)
}

// Perform health check on all servers
pub async fn perform_health_check_all_servers(
) -> std::collections::HashMap<String, process::ServerHealth> {
	process::perform_health_check_all_servers().await
}

// Get comprehensive server status report
pub fn get_server_status_report(
) -> std::collections::HashMap<String, (process::ServerHealth, process::ServerRestartInfo)> {
	process::get_server_status_report()
}
