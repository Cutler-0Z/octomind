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

// MCP Protocol Implementation

use crate::config::McpConnectionType;
use crate::log_debug;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::sync::{Arc, RwLock};
use uuid;

// Modules
pub mod tool_map;

// Cache for internal server function definitions (static during session)
lazy_static::lazy_static! {
	static ref INTERNAL_FUNCTION_CACHE: Arc<RwLock<std::collections::HashMap<String, Vec<McpFunction>>>> =
		Arc::new(RwLock::new(std::collections::HashMap::new()));
}

pub mod agent;
pub mod dev;
pub mod fs;
pub mod health_monitor;
pub mod process;
pub mod server;
pub mod web;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
	pub tool_name: String,
	pub parameters: Value,
	#[serde(default)]
	pub tool_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
	pub tool_name: String,
	pub result: Value,
	#[serde(default)]
	pub tool_id: String,
}

// MCP Protocol-compliant result creation helpers
impl McpToolResult {
	// Create a successful MCP result with text content
	pub fn success(tool_name: String, tool_id: String, content: String) -> Self {
		Self {
			tool_name,
			tool_id,
			result: json!({
				"content": [
					{
						"type": "text",
						"text": content
					}
				],
				"isError": false
			}),
		}
	}

	// Create a successful MCP result with rich content (includes metadata)
	pub fn success_with_metadata(
		tool_name: String,
		tool_id: String,
		content: String,
		metadata: serde_json::Value,
	) -> Self {
		Self {
			tool_name,
			tool_id,
			result: json!({
				"content": [
					{
						"type": "text",
						"text": content
					}
				],
				"isError": false,
				"metadata": metadata
			}),
		}
	}

	// Create an error MCP result
	pub fn error(tool_name: String, tool_id: String, error_message: String) -> Self {
		Self {
			tool_name,
			tool_id,
			result: json!({
				"content": [
					{
						"type": "text",
						"text": error_message
					}
				],
				"isError": true
			}),
		}
	}
}

// Extract content from MCP-compliant result
pub fn extract_mcp_content(result: &Value) -> String {
	// MCP Standard: Extract from content array
	if let Some(content_array) = result.get("content") {
		if let Some(content_items) = content_array.as_array() {
			let main_content = content_items
				.iter()
				.filter_map(|item| {
					if item.get("type").and_then(|t| t.as_str()) == Some("text") {
						item.get("text").and_then(|t| t.as_str())
					} else {
						None
					}
				})
				.collect::<Vec<_>>()
				.join("\n");

			// For debug mode, also include metadata if available
			if let Some(metadata) = result.get("metadata") {
				if !metadata.is_null() {
					return format!(
						"{}\n\n[Metadata: {}]",
						main_content,
						serde_json::to_string_pretty(metadata).unwrap_or_default()
					);
				}
			}

			return main_content;
		}
	}

	// Fallback: Check for old "output" field for backward compatibility
	if let Some(output) = result.get("output") {
		if let Some(output_str) = output.as_str() {
			return output_str.to_string();
		}
	}

	// Last resort: serialize the whole result for debugging
	serde_json::to_string_pretty(result).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpFunction {
	pub name: String,
	pub description: String,
	pub parameters: Value,
}

// Guess the category of a tool based on its name
pub fn guess_tool_category(tool_name: &str) -> &'static str {
	match tool_name {
		"core" => "system",
		"text_editor" => "developer",
		"list_files" => "filesystem",
		"read_html" => "web",
		name if name.contains("file") || name.contains("editor") => "developer",
		name if name.contains("search") || name.contains("find") => "search",
		name if name.contains("image") || name.contains("photo") => "media",
		name if name.contains("web") || name.contains("http") => "web",
		name if name.contains("db") || name.contains("database") => "database",
		name if name.contains("browser") => "browser",
		name if name.contains("terminal") => "terminal",
		name if name.contains("video") => "video",
		name if name.contains("audio") => "audio",
		name if name.contains("location") || name.contains("map") => "location",
		name if name.contains("google") => "google",
		name if name.contains("weather") => "weather",
		name if name.contains("calculator") || name.contains("math") => "math",
		name if name.contains("news") => "news",
		name if name.contains("email") => "email",
		name if name.contains("calendar") => "calendar",
		name if name.contains("translate") => "translation",
		name if name.contains("github") => "github",
		name if name.contains("git") => "git",
		_ => "external",
	}
}

// Parse a model's response to extract tool calls - kept for backward compatibility
pub fn parse_tool_calls(_content: &str) -> Vec<McpToolCall> {
	// This function is kept for backward compatibility but is no longer used directly
	// as we now prefer to pass tool calls directly as structs
	Vec::new()
}

// Structure to represent tool responses for OpenAI/Claude format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponseMessage {
	pub role: String,
	pub tool_call_id: String,
	pub name: String,
	pub content: String,
}

// Convert tool results to proper messages
pub fn tool_results_to_messages(results: &[McpToolResult]) -> Vec<ToolResponseMessage> {
	let mut messages = Vec::new();

	for result in results {
		messages.push(ToolResponseMessage {
			role: "tool".to_string(),
			tool_call_id: result.tool_id.clone(),
			name: result.tool_name.clone(),
			content: serde_json::to_string(&result.result).unwrap_or_default(),
		});
	}

	messages
}

// Ensure tool calls have valid IDs
pub fn ensure_tool_call_ids(calls: &mut [McpToolCall]) {
	for call in calls.iter_mut() {
		if call.tool_id.is_empty() {
			call.tool_id = format!("tool_{}", uuid::Uuid::new_v4().simple());
		}
	}
}

// Initialize all servers for a specific mode/role ONCE at startup
pub async fn initialize_servers_for_role(config: &crate::config::Config) -> Result<()> {
	// Only initialize if MCP has any servers configured
	if config.mcp.servers.is_empty() {
		crate::log_debug!("No MCP servers configured for initialization");
		return Ok(());
	}

	let enabled_servers: Vec<crate::config::McpServerConfig> = config.mcp.servers.to_vec();

	crate::log_debug!(
		"Initializing {} MCP servers at startup",
		enabled_servers.len()
	);

	for server in &enabled_servers {
		// Only initialize external servers that need to be started
		if let McpConnectionType::Http | McpConnectionType::Stdin = server.connection_type() {
			crate::log_debug!("Initializing external server: {}", server.name());

			// Check if server is already running to avoid double initialization
			if server::is_server_already_running_with_config(server) {
				crate::log_debug!(
					"Server '{}' is already running - skipping initialization",
					server.name()
				);
				continue;
			}

			// Start the server and cache its functions
			match server::get_server_functions(server).await {
				Ok(functions) => {
					crate::log_debug!(
						"Successfully initialized server '{}' with {} functions",
						server.name(),
						functions.len()
					);
					for func in &functions {
						crate::log_debug!("  - Available: {}", func.name);
					}
				}
				Err(e) => {
					crate::log_debug!(
						"Failed to initialize server '{}': {} (will retry on first use)",
						server.name(),
						e
					);
					// Don't fail startup - just log and continue
				}
			}
		} else {
			// Internal servers (Developer/Filesystem) don't need initialization
			crate::log_debug!(
				"Skipping initialization for internal server: {} ({:?})",
				server.name(),
				server.connection_type()
			);
		}
	}

	// Start the health monitor for external servers
	let config_arc = std::sync::Arc::new(config.clone());
	if let Err(e) = health_monitor::start_health_monitor(config_arc).await {
		crate::log_debug!("Failed to start health monitor: {}", e);
		// Don't fail startup - health monitoring is optional
	}

	crate::log_debug!("MCP server initialization completed");
	Ok(())
}

// Gather available functions from enabled servers WITHOUT spawning servers
// This is used for system prompt generation and should be fast
pub async fn get_available_functions(config: &crate::config::Config) -> Vec<McpFunction> {
	let mut functions = Vec::new();

	// Only gather functions if MCP has any servers configured
	if config.mcp.servers.is_empty() {
		crate::log_debug!("MCP has no servers configured, no functions available");
		return functions;
	}

	// Get enabled servers from the merged config (which should already be filtered by server_refs)
	let enabled_servers: Vec<crate::config::McpServerConfig> = config.mcp.servers.to_vec();

	for server in enabled_servers {
		match server.connection_type() {
			McpConnectionType::Builtin => {
				match server.name() {
					"developer" => {
						let server_functions =
							get_cached_internal_functions("developer", server.tools(), || {
								dev::get_all_functions()
							});
						functions.extend(server_functions);
					}
					"filesystem" => {
						let server_functions =
							get_cached_internal_functions("filesystem", server.tools(), || {
								fs::get_all_functions()
							});
						functions.extend(server_functions);
					}
					"agent" => {
						// For agent server, get all agent functions based on config
						// Don't cache agent functions since they depend on config
						let server_functions = agent::get_all_functions(config);
						let filtered_functions =
							filter_tools_by_patterns(server_functions, server.tools());
						functions.extend(filtered_functions);
					}
					"web" => {
						let server_functions =
							get_cached_internal_functions("web", server.tools(), || {
								web::get_all_functions()
							});
						functions.extend(server_functions);
					}
					_ => {
						// Unknown builtin server
						crate::log_debug!("Unknown builtin server: {}", server.name());
					}
				}
			}
			McpConnectionType::Http | McpConnectionType::Stdin => {
				// CRITICAL FIX: For external servers, use cached function discovery
				// This avoids spawning servers during system prompt creation
				match server::get_server_functions_cached(&server).await {
					Ok(server_functions) => {
						let filtered_functions =
							filter_tools_by_patterns(server_functions, server.tools());
						functions.extend(filtered_functions);
					}
					Err(e) => {
						crate::log_error!(
							"Failed to get cached functions from external server '{}': {} (will be available when server starts)",
							server.name(),
							e
						);
						// Don't fail - just continue without this server's functions
					}
				}
			}
		}
	}

	functions
}

// Helper function to filter tools based on patterns
fn filter_tools_by_patterns(tools: Vec<McpFunction>, allowed_tools: &[String]) -> Vec<McpFunction> {
	if allowed_tools.is_empty() {
		tools
	} else {
		tools
			.into_iter()
			.filter(|func| is_tool_allowed_by_patterns(&func.name, allowed_tools))
			.collect()
	}
}

// Helper function to check if a tool matches allowed patterns
pub fn is_tool_allowed_by_patterns(tool_name: &str, allowed_tools: &[String]) -> bool {
	if allowed_tools.is_empty() {
		return true;
	}

	for pattern in allowed_tools {
		// Handle wildcard patterns
		if pattern.ends_with('*') {
			let prefix = &pattern[..pattern.len() - 1];
			if tool_name.starts_with(prefix) {
				return true;
			}
		} else {
			// Exact match
			if tool_name == pattern {
				return true;
			}
		}
	}

	false
}

// Helper function to get cached internal functions with filtering
pub fn get_cached_internal_functions<F>(
	server_type: &str,
	allowed_tools: &[String],
	get_functions: F,
) -> Vec<McpFunction>
where
	F: FnOnce() -> Vec<McpFunction>,
{
	let cache_key = if allowed_tools.is_empty() {
		format!("{}_all", server_type)
	} else {
		format!("{}_{}", server_type, allowed_tools.join(","))
	};

	// Try to get from cache first
	{
		let cache = INTERNAL_FUNCTION_CACHE.read().unwrap();
		if let Some(cached_functions) = cache.get(&cache_key) {
			return cached_functions.clone();
		}
	}

	// Not in cache - compute and cache
	crate::log_debug!("Computing and caching {} functions", server_type);
	let all_functions = get_functions();
	let filtered_functions = if allowed_tools.is_empty() {
		all_functions
	} else {
		all_functions
			.into_iter()
			.filter(|func| is_tool_allowed_by_patterns(&func.name, allowed_tools))
			.collect()
	};

	// Cache the result
	{
		let mut cache = INTERNAL_FUNCTION_CACHE.write().unwrap();
		cache.insert(cache_key, filtered_functions.clone());
	}

	filtered_functions
}

// Clear internal function cache (useful for testing or when tools configuration changes)
pub fn clear_internal_function_cache() {
	let mut cache = INTERNAL_FUNCTION_CACHE.write().unwrap();
	let count = cache.len();
	cache.clear();
	if count > 0 {
		crate::log_debug!("Cleared internal function cache for {} entries", count);
	}
}

// Execute a tool call
pub async fn execute_tool_call(
	call: &McpToolCall,
	config: &crate::config::Config,
	cancellation_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(McpToolResult, u64)> {
	use std::sync::atomic::Ordering;

	// Debug logging for tool execution
	log_debug!("Debug: Executing tool call: {}", call.tool_name);
	log_debug!(
		"Debug: MCP has {} servers configured",
		config.mcp.servers.len()
	);
	if let Ok(params) = serde_json::to_string_pretty(&call.parameters) {
		log_debug!("Debug: Tool parameters: {}", params);
	}

	// Only execute if MCP has any servers configured
	if config.mcp.servers.is_empty() {
		return Err(anyhow::anyhow!("MCP has no servers configured"));
	}

	// Check for cancellation before starting
	if let Some(ref token) = cancellation_token {
		if token.load(Ordering::SeqCst) {
			return Err(anyhow::anyhow!("Tool execution cancelled"));
		}
	}

	// Track tool execution time
	let tool_start = std::time::Instant::now();

	let result = try_execute_tool_call(call, config, cancellation_token.clone()).await;

	// Calculate tool execution time
	let tool_duration = tool_start.elapsed();
	let tool_time_ms = tool_duration.as_millis() as u64;

	match result {
		Ok(tool_result) => {
			// Apply large response handling to ALL tools in one centralized place
			let checked_result = handle_large_response(tool_result, config).await?;
			Ok((checked_result, tool_time_ms))
		}
		Err(e) => Err(e),
	}
}

// Build a simple tool-to-server lookup map for instant routing
pub async fn build_tool_server_map(
	config: &crate::config::Config,
) -> std::collections::HashMap<String, crate::config::McpServerConfig> {
	let mut tool_map = std::collections::HashMap::new();
	let enabled_servers: Vec<crate::config::McpServerConfig> = config.mcp.servers.to_vec();

	for server in enabled_servers {
		// Get all functions this server provides
		let server_functions = match server.connection_type() {
			McpConnectionType::Builtin => {
				match server.name() {
					"developer" => {
						// Developer server only has shell and other dev tools (agent moved to separate server)
						get_cached_internal_functions("developer", server.tools(), || {
							dev::get_all_functions()
						})
					}
					"filesystem" => {
						get_cached_internal_functions("filesystem", server.tools(), || {
							fs::get_all_functions()
						})
					}
					"agent" => {
						// For agent server, get all agent functions based on config
						// Don't cache agent functions since they depend on config
						let server_functions = agent::get_all_functions(config);
						filter_tools_by_patterns(server_functions, server.tools())
					}
					"web" => get_cached_internal_functions("web", server.tools(), || {
						web::get_all_functions()
					}),
					_ => {
						crate::log_debug!("Unknown builtin server: {}", server.name());
						Vec::new()
					}
				}
			}
			McpConnectionType::Http | McpConnectionType::Stdin => {
				// For external servers, get their actual functions
				match server::get_server_functions_cached(&server).await {
					Ok(functions) => filter_tools_by_patterns(functions, server.tools()),
					Err(_) => Vec::new(), // Server not available, skip
				}
			}
		};

		// Map each function name to this server
		for function in server_functions {
			// CONFIGURATION ORDER PRIORITY: First server wins for each tool
			tool_map
				.entry(function.name)
				.or_insert_with(|| server.clone());
		}
	}

	crate::log_debug!("Built tool-to-server map with {} tools", tool_map.len());
	tool_map
}

// Internal function to actually execute the tool call with cancellation support
async fn try_execute_tool_call(
	call: &McpToolCall,
	config: &crate::config::Config,
	cancellation_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<McpToolResult> {
	use std::sync::atomic::Ordering;

	// Only execute if MCP has any servers configured
	if config.mcp.servers.is_empty() {
		return Err(anyhow::anyhow!("MCP has no servers configured"));
	}

	// Check for cancellation before proceeding
	if let Some(ref token) = cancellation_token {
		if token.load(Ordering::SeqCst) {
			return Err(anyhow::anyhow!("Tool execution cancelled"));
		}
	}

	// STATIC ROUTING: Use pre-built tool map ONLY
	let tool_server_map = {
		let mut map = std::collections::HashMap::new();
		if let Some(server) = tool_map::get_server_for_tool(&call.tool_name) {
			map.insert(call.tool_name.clone(), server);
		}
		map
	};

	// Find the server that provides this tool
	if let Some(target_server) = tool_server_map.get(&call.tool_name) {
		crate::log_debug!(
			"Routing tool '{}' to server '{}' ({:?})",
			call.tool_name,
			target_server.name(),
			target_server.connection_type()
		);

		// Check for cancellation before execution
		if let Some(ref token) = cancellation_token {
			if token.load(Ordering::SeqCst) {
				return Err(anyhow::anyhow!("Tool execution cancelled"));
			}
		}

		// Execute on the target server
		match target_server.connection_type() {
			McpConnectionType::Builtin => {
				match target_server.name() {
					"developer" => match call.tool_name.as_str() {
						"shell" => {
							crate::log_debug!(
								"Executing shell command via developer server '{}'",
								target_server.name()
							);
							let mut result =
								dev::execute_shell_command(call, cancellation_token.clone())
									.await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						_ => {
							return Err(anyhow::anyhow!(
								"Tool '{}' not implemented in developer server",
								call.tool_name
							));
						}
					},
					"filesystem" => match call.tool_name.as_str() {
						"text_editor" => {
							crate::log_debug!(
								"Executing text_editor via filesystem server '{}'",
								target_server.name()
							);
							let mut result =
								fs::execute_text_editor(call, cancellation_token.clone()).await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						"list_files" => {
							crate::log_debug!(
								"Executing list_files via filesystem server '{}'",
								target_server.name()
							);
							let mut result =
								fs::execute_list_files(call, cancellation_token.clone()).await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						_ => {
							return Err(anyhow::anyhow!(
								"Tool '{}' not implemented in filesystem server",
								call.tool_name
							));
						}
					},
					"agent" => {
						// Handle any agent tool (agent_<name>)
						if call.tool_name.starts_with("agent_") {
							crate::log_debug!(
								"Executing agent command '{}' via agent server '{}'",
								call.tool_name,
								target_server.name()
							);
							let mut result = agent::execute_agent_command(
								call,
								config,
								cancellation_token.clone(),
							)
							.await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						} else {
							return Err(anyhow::anyhow!(
								"Tool '{}' not implemented in agent server",
								call.tool_name
							));
						}
					}
					"web" => match call.tool_name.as_str() {
						"web_search" => {
							crate::log_debug!(
								"Executing web_search via web server '{}'",
								target_server.name()
							);
							let mut result =
								web::execute_web_search(call, cancellation_token.clone()).await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						"image_search" => {
							crate::log_debug!(
								"Executing image_search via web server '{}'",
								target_server.name()
							);
							let mut result =
								web::execute_image_search(call, cancellation_token.clone()).await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						"video_search" => {
							crate::log_debug!(
								"Executing video_search via web server '{}'",
								target_server.name()
							);
							let mut result =
								web::execute_video_search(call, cancellation_token.clone()).await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						"news_search" => {
							crate::log_debug!(
								"Executing news_search via web server '{}'",
								target_server.name()
							);
							let mut result =
								web::execute_news_search(call, cancellation_token.clone()).await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						"read_html" => {
							crate::log_debug!(
								"Executing read_html via web server '{}'",
								target_server.name()
							);
							let mut result =
								web::execute_read_html(call, cancellation_token.clone()).await?;
							result.tool_id = call.tool_id.clone();
							return Ok(result);
						}
						_ => {
							return Err(anyhow::anyhow!(
								"Tool '{}' not implemented in web server",
								call.tool_name
							));
						}
					},
					_ => {
						return Err(anyhow::anyhow!(
							"Unknown builtin server: {}",
							target_server.name()
						));
					}
				}
			}
			McpConnectionType::Http | McpConnectionType::Stdin => {
				// Execute on external server
				match server::execute_tool_call(call, target_server, cancellation_token.clone())
					.await
				{
					Ok(mut result) => {
						result.tool_id = call.tool_id.clone();
						return Ok(result);
					}
					Err(err) => {
						return Err(err);
					}
				}
			}
		}
	}

	// If we get here, tool was not found in any server
	let available_tools = tool_map::get_all_tool_names();
	Err(anyhow::anyhow!(
		"Tool '{}' not found in any configured MCP server. Available tools: {}",
		call.tool_name,
		if available_tools.is_empty() {
			"none (tool map not initialized)".to_string()
		} else {
			available_tools.join(", ")
		}
	))
}

// Helper function to handle large response warnings
async fn handle_large_response(
	result: McpToolResult,
	config: &crate::config::Config,
) -> Result<McpToolResult> {
	// Check if result is large - warn user if it exceeds threshold
	let estimated_tokens = crate::session::estimate_tokens(&format!("{}", result.result));
	if estimated_tokens > config.mcp_response_warning_threshold {
		// Create a modified result that warns about the size
		use colored::Colorize;

		// Get server name for better identification
		let server_name =
			crate::session::chat::response::get_tool_server_name_async(&result.tool_name, config)
				.await;

		println!(
			"{}",
			format!(
				"! WARNING: Tool '{}' ({}){} produced a large output ({} tokens)",
				result.tool_name,
				server_name,
				if !result.tool_id.is_empty() {
					format!(" [ID: {}]", result.tool_id)
				} else {
					String::new()
				},
				estimated_tokens
			)
			.bright_yellow()
		);
		println!(
			"{}",
			"This may consume significant tokens and impact your usage limits.".bright_yellow()
		);

		// Ask user for confirmation before proceeding
		print!(
			"{}",
			"Do you want to continue with this large output? [y/N]: ".bright_cyan()
		);
		std::io::stdout().flush().unwrap();

		let mut input = String::new();
		std::io::stdin().read_line(&mut input).unwrap_or_default();

		if !input.trim().to_lowercase().starts_with('y') {
			// User declined large output. Return an MCP-compliant error result instead of
			// breaking the communication flow. This allows the conversation to continue
			// normally while informing the AI that the user declined the large output.
			println!(
				"{}",
				format!(
					"Large output from '{}' ({}) declined by user. Continuing conversation...",
					result.tool_name, server_name
				)
				.bright_red()
			);
			return Ok(McpToolResult::error(
				result.tool_name.clone(),
				result.tool_id.clone(),
				format!("User declined to process large output from tool '{}' ({} tokens). The tool executed successfully but the output was too large and the user chose not to include it in the conversation to avoid excessive token usage.", result.tool_name, estimated_tokens)
			));
		}

		// User confirmed, continue with original result
		println!("{}", "Proceeding with full output...".bright_green());
	}

	Ok(result)
}

// Execute a tool call with layer-specific restrictions
pub async fn execute_layer_tool_call(
	call: &McpToolCall,
	config: &crate::config::Config,
	layer_config: &crate::session::layers::LayerConfig,
) -> Result<(McpToolResult, u64)> {
	// Check if tools are enabled for this layer (has server_refs)
	if layer_config.mcp.server_refs.is_empty() {
		return Err(anyhow::anyhow!("Tool execution is disabled for this layer"));
	}

	// Check if specific tool is allowed for this layer
	if !layer_config.mcp.allowed_tools.is_empty()
		&& !layer_config.mcp.allowed_tools.contains(&call.tool_name)
	{
		return Err(anyhow::anyhow!(
			"Tool '{}' is not allowed for this layer",
			call.tool_name
		));
	}

	// Pass to regular tool execution
	execute_tool_call(call, config, None).await
}

// Execute multiple tool calls
pub async fn execute_tool_calls(
	calls: &[McpToolCall],
	config: &crate::config::Config,
) -> Vec<Result<(McpToolResult, u64)>> {
	let mut results = Vec::new();

	for call in calls {
		// Execute the tool call
		let result = execute_tool_call(call, config, None).await;
		results.push(result);
	}

	results
}
