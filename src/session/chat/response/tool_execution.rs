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

// Tool execution module - handles parallel tool execution, display, and error handling
// Unified interface for both main sessions and layers

use crate::config::Config;
use crate::session::chat::session::ChatSession;
use crate::session::chat::ToolProcessor;
use crate::{log_debug, log_info};
use anyhow::Result;
use colored::Colorize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Context for tool execution - can be either main session or layer context
pub enum ToolExecutionContext<'a> {
	/// Main session context with full session access
	MainSession {
		chat_session: &'a mut ChatSession,
		tool_processor: &'a mut ToolProcessor,
	},
	/// Layer context with layer-specific configuration
	Layer {
		session_name: String,
		layer_config: &'a crate::session::layers::LayerConfig,
		layer_name: String,
	},
}

impl ToolExecutionContext<'_> {
	/// Get session name for logging
	pub fn session_name(&self) -> &str {
		match self {
			ToolExecutionContext::MainSession { chat_session, .. } => {
				&chat_session.session.info.name
			}
			ToolExecutionContext::Layer { session_name, .. } => session_name,
		}
	}

	/// Check if tool is allowed in this context
	pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
		match self {
			ToolExecutionContext::MainSession { .. } => true, // Main session allows all tools
			ToolExecutionContext::Layer { layer_config, .. } => {
				layer_config.mcp.allowed_tools.is_empty()
					|| layer_config
						.mcp
						.allowed_tools
						.contains(&tool_name.to_string())
			}
		}
	}

	/// Get error tracker (if available)
	pub fn error_tracker(
		&mut self,
	) -> Option<&mut crate::session::chat::tool_error_tracker::ToolErrorTracker> {
		match self {
			ToolExecutionContext::MainSession { tool_processor, .. } => {
				Some(&mut tool_processor.error_tracker)
			}
			ToolExecutionContext::Layer { .. } => None, // Layers don't have error tracking yet
		}
	}

	/// Increment tool call counter
	pub fn increment_tool_calls(&mut self) {
		if let ToolExecutionContext::MainSession { chat_session, .. } = self {
			chat_session.session.info.tool_calls += 1;
		}
	}

	/// Handle declined output by removing tool call from conversation
	pub fn handle_declined_output(&mut self, tool_id: &str) {
		if let ToolExecutionContext::MainSession { chat_session, .. } = self {
			handle_declined_output_internal(tool_id, chat_session);
		}
		// For layers, we don't need to modify conversation history
	}
}

/// Execute all tool calls in parallel and collect results - unified interface
pub async fn execute_tools_parallel_unified(
	current_tool_calls: Vec<crate::mcp::McpToolCall>,
	context: &mut ToolExecutionContext<'_>,
	config: &Config,
	operation_cancelled: Option<Arc<AtomicBool>>,
) -> Result<(Vec<crate::mcp::McpToolResult>, u64)> {
	let operation_cancelled =
		operation_cancelled.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

	// Filter tools based on context permissions
	let allowed_tool_calls: Vec<_> = current_tool_calls
		.into_iter()
		.filter(|tool_call| {
			if context.is_tool_allowed(&tool_call.tool_name) {
				true
			} else {
				println!(
					"{} {} {}",
					"Tool".red(),
					tool_call.tool_name,
					"not allowed in this context".red()
				);
				false
			}
		})
		.collect();

	if allowed_tool_calls.is_empty() {
		return Ok((Vec::new(), 0));
	}

	execute_tools_parallel_internal(allowed_tool_calls, context, config, operation_cancelled).await
}

// Execute all tool calls in parallel and collect results (legacy interface for main session)
pub async fn execute_tools_parallel(
	current_tool_calls: Vec<crate::mcp::McpToolCall>,
	chat_session: &mut ChatSession,
	config: &Config,
	tool_processor: &mut ToolProcessor,
	operation_cancelled: Arc<AtomicBool>,
) -> Result<(Vec<crate::mcp::McpToolResult>, u64)> {
	let mut context = ToolExecutionContext::MainSession {
		chat_session,
		tool_processor,
	};

	let result = execute_tools_parallel_unified(
		current_tool_calls.clone(),
		&mut context,
		config,
		Some(operation_cancelled),
	)
	.await;

	// CRITICAL FIX: Ensure conversation state integrity after tool execution
	// Fix the assistant message's tool_calls field to match actual tool results
	// This must run regardless of success/failure to handle Ctrl+C cancellations
	let tool_results = match &result {
		Ok((results, _)) => results.as_slice(),
		Err(_) => &[], // No results when all tools failed/cancelled
	};
	fix_assistant_message_tool_calls(&current_tool_calls, tool_results, chat_session);

	result
}

// Internal implementation that works with the unified context
async fn execute_tools_parallel_internal(
	current_tool_calls: Vec<crate::mcp::McpToolCall>,
	context: &mut ToolExecutionContext<'_>,
	config: &Config,
	operation_cancelled: Arc<AtomicBool>,
) -> Result<(Vec<crate::mcp::McpToolResult>, u64)> {
	let mut tool_tasks = Vec::new();
	let is_single_tool = current_tool_calls.len() == 1;

	for (index, tool_call) in current_tool_calls.clone().iter().enumerate() {
		// Increment tool call counter
		context.increment_tool_calls();

		// CRITICAL FIX: Use the EXACT tool_id from the original API response
		// Don't generate a new UUID - use the one from the original tool_calls
		let original_tool_id = tool_call.tool_id.clone();

		// Clone tool_name separately for tool task tracking
		let tool_name = tool_call.tool_name.clone();
		let tool_index = index + 1; // 1-based index for display

		// Execute in a tokio task
		let config_clone = config.clone();
		let params_clone = tool_call.parameters.clone();

		// Log the tool request with the session name and ORIGINAL tool_id
		let _ = crate::session::logger::log_tool_call(
			context.session_name(),
			&tool_name,
			&original_tool_id,
			&params_clone,
		);

		let tool_id_for_task = original_tool_id.clone();
		let tool_call_clone = tool_call.clone(); // Clone for async move
		let cancel_token_for_task = operation_cancelled.clone(); // Pass cancellation token

		// Create the appropriate execution task based on context
		let task = match context {
			ToolExecutionContext::MainSession { .. } => {
				tokio::spawn(async move {
					let mut call_with_id = tool_call_clone.clone();
					// CRITICAL: Use the original tool_id, don't change it
					call_with_id.tool_id = tool_id_for_task.clone();
					crate::mcp::execute_tool_call(
						&call_with_id,
						&config_clone,
						Some(cancel_token_for_task),
					)
					.await
				})
			}
			ToolExecutionContext::Layer { layer_config, .. } => {
				let layer_config_clone = layer_config.clone();
				tokio::spawn(async move {
					let mut call_with_id = tool_call_clone.clone();
					// CRITICAL: Use the original tool_id, don't change it
					call_with_id.tool_id = tool_id_for_task.clone();
					crate::mcp::execute_layer_tool_call(
						&call_with_id,
						&config_clone,
						&layer_config_clone,
					)
					.await
				})
			}
		};

		tool_tasks.push((tool_name, task, original_tool_id, tool_index));
	}

	// FIXED: Proper parallel awaiting with immediate cancellation support
	let mut tool_results = Vec::new();
	let mut _has_error = false;
	let mut total_tool_time_ms = 0; // Track cumulative tool execution time

	// Extract task info for later use
	let task_info: Vec<(String, String, usize)> = tool_tasks
		.iter()
		.map(|(tool_name, _, tool_id, tool_index)| {
			(tool_name.clone(), tool_id.clone(), *tool_index)
		})
		.collect();

	// Extract just the tasks for parallel execution
	let tasks: Vec<_> = tool_tasks.into_iter().map(|(_, task, _, _)| task).collect();

	// Use tokio::select! for immediate cancellation response
	tokio::select! {
		task_results = futures::future::join_all(tasks) => {
			// All tasks completed before cancellation
			for ((tool_name, tool_id, tool_index), task_result) in task_info.into_iter().zip(task_results) {
				// Store tool call info for consolidated display after execution
				let tool_call_info = current_tool_calls
					.iter()
					.find(|tc| tc.tool_id == tool_id)
					.or_else(|| {
						current_tool_calls
							.iter()
							.find(|tc| tc.tool_name == tool_name)
					});

				// Store for display after execution
				let stored_tool_call = tool_call_info.cloned();

				match task_result {
			Ok(result) => match result {
				Ok((res, tool_time_ms)) => {
					// Tool succeeded, reset the error counter (if available)
					if let Some(error_tracker) = context.error_tracker() {
						error_tracker.record_success(&tool_name);
					}

					// Display the complete tool execution with consolidated info
					let display_params = ToolDisplayParams {
						stored_tool_call: &stored_tool_call,
						tool_name: &tool_name,
						tool_id: &tool_id,
						tool_index,
						is_single_tool,
					};
					display_tool_success(
						display_params,
						&res,
						tool_time_ms,
						config,
						context.session_name(),
					)
					.await;

					tool_results.push(res);
					// Accumulate tool execution time
					total_tool_time_ms += tool_time_ms;
				}
				Err(e) => {
					_has_error = true;

					// Check if this is a user-declined large output error
					if e.to_string().contains("LARGE_OUTPUT_DECLINED_BY_USER") {
						context.handle_declined_output(&tool_id);
						continue;
					}

					// Display error in consolidated format for other errors
					display_tool_error(&stored_tool_call, &tool_name, &e, tool_index, is_single_tool);

					// Track errors for this tool (if error tracking is available)
					let loop_detected = if let Some(error_tracker) = context.error_tracker() {
						error_tracker.record_error(&tool_name)
					} else {
						false
					};

					if loop_detected {
						// Always show loop detection warning since it's critical
						if let Some(error_tracker) = context.error_tracker() {
							println!("{}", format!("⚠ Warning: {} failed {} times in a row - AI should try a different approach",
								tool_name, error_tracker.max_consecutive_errors()).bright_yellow());

							// Add a detailed error result for loop detection
							let loop_error_result = crate::mcp::McpToolResult {
								tool_name: tool_name.clone(),
								tool_id: tool_id.clone(),
								result: serde_json::json!({
									"error": format!("LOOP DETECTED: Tool '{}' failed {} consecutive times. Last error: {}. Please try a completely different approach or ask the user for guidance.", tool_name, error_tracker.max_consecutive_errors(), e),
									"tool_name": tool_name,
									"consecutive_failures": error_tracker.max_consecutive_errors(),
									"loop_detected": true,
									"suggestion": "Try a different tool or approach, or ask user for clarification"
								}),
							};
							tool_results.push(loop_error_result);
						}
					} else {
						// Regular error - add normal error result
						let error_result = if let Some(error_tracker) = context.error_tracker() {
							crate::mcp::McpToolResult {
								tool_name: tool_name.clone(),
								tool_id: tool_id.clone(),
								result: serde_json::json!({
									"error": format!("Tool execution failed: {}", e),
									"tool_name": tool_name,
									"attempt": error_tracker.get_error_count(&tool_name),
									"max_attempts": error_tracker.max_consecutive_errors()
								}),
							}
						} else {
							// For layers without error tracking
							crate::mcp::McpToolResult {
								tool_name: tool_name.clone(),
								tool_id: tool_id.clone(),
								result: serde_json::json!({
									"error": format!("Tool execution failed: {}", e),
									"tool_name": tool_name,
								}),
							}
						};
						tool_results.push(error_result);

						if let Some(error_tracker) = context.error_tracker() {
							log_info!(
								"Tool '{}' failed {} of {} times. Adding error to context.",
								tool_name,
								error_tracker.get_error_count(&tool_name),
								error_tracker.max_consecutive_errors()
							);
						}
					}
				}
			},
			Err(e) => {
				_has_error = true;

				// Check if this is a user-declined large output error (can occur at task level too)
				if e.to_string().contains("LARGE_OUTPUT_DECLINED_BY_USER") {
					context.handle_declined_output(&tool_id);
					continue;
				}

				// Display task error in consolidated format for other errors
				display_tool_error(&stored_tool_call, &tool_name, &anyhow::anyhow!("{}", e), tool_index, is_single_tool);

				// Show task error status
				println!("✗ Task error for '{}': {}", tool_name, e);

				// ALWAYS add error result for task failures too (unless it was a user decline)
				let error_result = crate::mcp::McpToolResult {
					tool_name: tool_name.clone(),
					tool_id: tool_id.clone(),
					result: serde_json::json!({
						"error": format!("Internal task error: {}", e),
						"tool_name": tool_name,
						"error_type": "task_failure"
					}),
				};
				tool_results.push(error_result);
			}
		}
			}
		},
		_ = async {
			loop {
				if operation_cancelled.load(Ordering::SeqCst) {
					break;
				}
				tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
			}
		} => {
			// Cancellation occurred - provide immediate feedback
			use colored::*;
			println!(
				"{}",
				"🛑 All tool execution cancelled - returning to input".bright_yellow()
			);

			// Show cancellation message for each tool
			for (tool_name, _, _) in task_info {
				println!(
					"{}",
					format!("🛑 Tool '{}' cancelled - server preserved", tool_name).bright_yellow()
				);
			}

			// Return empty results for cancelled execution
			return Ok((Vec::new(), total_tool_time_ms));
		}
	}

	Ok((tool_results, total_tool_time_ms))
}

// Parameters for tool display functions
struct ToolDisplayParams<'a> {
	stored_tool_call: &'a Option<crate::mcp::McpToolCall>,
	tool_name: &'a str,
	tool_id: &'a str,
	tool_index: usize,
	is_single_tool: bool,
}

// Display successful tool execution (after execution - header + results)
async fn display_tool_success(
	params: ToolDisplayParams<'_>,
	res: &crate::mcp::McpToolResult,
	tool_time_ms: u64,
	config: &Config,
	session_name: &str,
) {
	// For multiple tools: show header again with index
	// For single tool: skip header (already shown upfront)
	if !params.is_single_tool {
		display_individual_tool_header_with_params(
			params.tool_name,
			params.stored_tool_call,
			config,
			params.tool_index,
		)
		.await;
	}

	// Show the actual tool output based on log level using MCP protocol
	if config.get_log_level().is_info_enabled() || config.get_log_level().is_debug_enabled() {
		// Extract content using MCP protocol
		let content = crate::mcp::extract_mcp_content(&res.result);

		if !content.trim().is_empty() {
			if config.get_log_level().is_debug_enabled() {
				// Debug mode: Show full content
				println!("{}", content);
			} else {
				// Info mode: Show smart output (with some reasonable limits)
				display_tool_output_smart(&content);
			}
		}
	}
	// None mode: No output shown (as requested)

	// Always show completion status with timing
	println!(
		"✓ Tool '{}' completed in {}ms",
		params.tool_name, tool_time_ms
	);
	println!("──────────────────");

	// Log the tool response with session name and timing
	let _ = crate::session::logger::log_tool_result(
		session_name,
		params.tool_id,
		&res.result,
		tool_time_ms,
	);
}

/// Display individual tool header with parameters (for parallel execution results)
async fn display_individual_tool_header_with_params(
	tool_name: &str,
	stored_tool_call: &Option<crate::mcp::McpToolCall>,
	config: &Config,
	tool_index: usize,
) {
	use colored::Colorize;

	// Get server name using same logic as execution
	let server_name =
		crate::session::chat::response::get_tool_server_name_async(tool_name, config).await;

	// Create formatted header matching the original style with index
	let title = format!(
		" [{}] {} | {} ",
		tool_index,
		tool_name.bright_cyan(),
		server_name.bright_blue()
	);
	let separator_length = 70.max(title.len() + 4);
	let dashes = "─".repeat(separator_length - title.len());
	let separator = format!("──{}{}──", title, dashes.dimmed());
	println!("{}", separator);

	// Show parameters if available and log level allows
	if let Some(tool_call) = stored_tool_call {
		if config.get_log_level().is_info_enabled() || config.get_log_level().is_debug_enabled() {
			crate::session::chat::response::display_tool_parameters_full(tool_call, config);
			println!(); // Extra newline after parameters for better spacing
		}
	}
}

// Display tool output in smart format (for info mode)
fn display_tool_output_smart(output_str: &str) {
	let lines: Vec<&str> = output_str.lines().collect();

	if lines.len() <= 20 && output_str.chars().count() <= 2000 {
		// Small output: show as-is
		println!("{}", output_str);
	} else if lines.len() > 20 {
		// Many lines: show first 15 lines + summary
		for line in lines.iter().take(15) {
			println!("{}", line);
		}
		println!("... [{} more lines]", lines.len().saturating_sub(15));
	} else {
		// Long single line or few long lines: truncate
		let truncated: String = output_str.chars().take(1997).collect();
		println!("{}...", truncated);
	}
}

// Display tool error in consolidated format
fn display_tool_error(
	stored_tool_call: &Option<crate::mcp::McpToolCall>,
	tool_name: &str,
	error: &anyhow::Error,
	tool_index: usize,
	is_single_tool: bool,
) {
	// For multiple tools: show header again with index
	// For single tool: skip header (already shown upfront)
	if !is_single_tool {
		if let Some(tool_call) = stored_tool_call {
			let category = crate::mcp::guess_tool_category(&tool_call.tool_name);
			let title = format!(
				" [{}] {} | {} ",
				tool_index,
				tool_call.tool_name.bright_cyan(),
				category.bright_blue()
			);
			let separator_length = 70.max(title.len() + 4);
			let dashes = "─".repeat(separator_length - title.len());
			let separator = format!("──{}{}──", title, dashes.dimmed());
			println!("{}", separator);
		}
	}

	// Show error status
	println!("✗ Tool '{}' failed: {}", tool_name, error);
}

// Handle user-declined large output (internal implementation)
fn handle_declined_output_internal(tool_id: &str, chat_session: &mut ChatSession) {
	println!("⚠ Tool output declined by user - removing tool call from conversation");

	// CRITICAL FIX: Remove the tool_use block from the assistant message
	// to prevent "tool_use ids found without tool_result blocks" error
	if let Some(last_msg) = chat_session.session.messages.last_mut() {
		if last_msg.role == "assistant" {
			if let Some(tool_calls_value) = &last_msg.tool_calls {
				// Parse the tool_calls and remove the declined one
				if let Ok(mut tool_calls_array) =
					serde_json::from_value::<Vec<serde_json::Value>>(tool_calls_value.clone())
				{
					// Remove the tool call with matching ID
					tool_calls_array
						.retain(|tc| tc.get("id").and_then(|id| id.as_str()) != Some(tool_id));

					// Update the assistant message
					if tool_calls_array.is_empty() {
						// No more tool calls, remove the tool_calls field entirely
						last_msg.tool_calls = None;
						log_debug!("Removed all tool calls from assistant message after user declined large output");
					} else {
						// Update with remaining tool calls
						last_msg.tool_calls =
							Some(serde_json::to_value(tool_calls_array).unwrap_or_default());
						log_debug!(
							"Removed declined tool call '{}' from assistant message",
							tool_id
						);
					}
				}
			}
		}
	}
}

/// Execute tool calls for layers using the unified parallel execution logic
pub async fn execute_layer_tool_calls_parallel(
	tool_calls: Vec<crate::mcp::McpToolCall>,
	session_name: String,
	layer_config: &crate::session::layers::LayerConfig,
	layer_name: String,
	config: &Config,
	operation_cancelled: Option<Arc<AtomicBool>>,
) -> Result<(Vec<crate::mcp::McpToolResult>, u64)> {
	let mut context = ToolExecutionContext::Layer {
		session_name,
		layer_config,
		layer_name,
	};

	execute_tools_parallel_unified(tool_calls, &mut context, config, operation_cancelled).await
}

/// CRITICAL FIX: Ensure conversation state integrity after tool execution
///
/// This function fixes the assistant message's tool_calls field to match actual tool results.
/// It prevents API errors like "tool_use ids found without tool_result blocks" by ensuring
/// every tool_use in the assistant message has a corresponding tool_result.
///
/// This is essential when tools are cancelled via Ctrl+C but some complete successfully.
fn fix_assistant_message_tool_calls(
	original_tool_calls: &[crate::mcp::McpToolCall],
	tool_results: &[crate::mcp::McpToolResult],
	chat_session: &mut ChatSession,
) {
	// Collect tool IDs that actually produced results
	let completed_tool_ids: std::collections::HashSet<String> = tool_results
		.iter()
		.map(|result| result.tool_id.clone())
		.collect();

	// Find the most recent assistant message with tool_calls
	if let Some(assistant_msg) = chat_session
		.session
		.messages
		.iter_mut()
		.rev() // Start from the end
		.find(|msg| msg.role == "assistant" && msg.tool_calls.is_some())
	{
		if let Some(tool_calls_value) = &assistant_msg.tool_calls {
			// Parse the existing tool_calls array
			if let Ok(mut tool_calls_array) =
				serde_json::from_value::<Vec<serde_json::Value>>(tool_calls_value.clone())
			{
				// Filter to keep only tool_calls that have corresponding results
				tool_calls_array.retain(|tc| {
					if let Some(tool_id) = tc.get("id").and_then(|id| id.as_str()) {
						completed_tool_ids.contains(tool_id)
					} else {
						false // Remove tool_calls without valid IDs
					}
				});

				// Update the assistant message based on remaining tool_calls
				if tool_calls_array.is_empty() {
					// No tools completed - remove tool_calls field entirely
					assistant_msg.tool_calls = None;
					log_debug!(
						"Removed all tool_calls from assistant message - no tools completed"
					);
				} else if tool_calls_array.len() < original_tool_calls.len() {
					// Some tools completed - update with filtered array
					let completed_count = tool_calls_array.len();
					assistant_msg.tool_calls =
						Some(serde_json::to_value(&tool_calls_array).unwrap_or_default());
					log_debug!(
						"Filtered tool_calls in assistant message: {} of {} tools completed",
						completed_count,
						original_tool_calls.len()
					);
				}
				// If all tools completed, no changes needed (normal flow)
			}
		}
	}
}
