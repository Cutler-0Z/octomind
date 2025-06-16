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

use super::super::layer_trait::{Layer, LayerConfig, LayerResult};
use crate::config::Config;
use crate::session::{Message, Session};
use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Generic layer implementation that can work with any layer configuration
/// This replaces the need for specific layer type implementations
pub struct GenericLayer {
	config: LayerConfig,
}

impl GenericLayer {
	pub fn new(config: LayerConfig) -> Self {
		Self { config }
	}

	/// Create messages for the API based on the layer configuration
	fn create_messages(&self, input: &str, session: &Session, session_model: &str) -> Vec<Message> {
		let mut messages = Vec::new();

		// Get the effective system prompt for this layer
		let system_prompt = self.config.get_effective_system_prompt();

		// Get the effective model for this layer
		let effective_model = self.config.get_effective_model(session_model);

		// Only mark system messages as cached if the model supports it
		let should_cache = crate::session::model_utils::model_supports_caching(&effective_model);

		messages.push(Message {
			role: "system".to_string(),
			content: system_prompt,
			timestamp: std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs(),
			cached: should_cache,
			tool_call_id: None,
			name: None,
			tool_calls: None,
			images: None,
		});

		// Prepare input based on input_mode using the trait's prepare_input method
		let processed_input = self.prepare_input(input, session);

		// Add user message with the processed input
		messages.push(Message {
			role: "user".to_string(),
			content: processed_input,
			timestamp: std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs(),
			cached: false,
			tool_call_id: None,
			name: None,
			tool_calls: None,
			images: None,
		});

		messages
	}

	/// Process recursive tool calls using the same logic as main sessions
	/// This ensures layers have full recursive tool call support
	#[allow(clippy::too_many_arguments)]
	async fn process_recursive_tool_calls(
		&self,
		initial_output: String,
		initial_exchange: crate::session::ProviderExchange,
		initial_tool_calls: Option<Vec<crate::mcp::McpToolCall>>,
		messages: Vec<Message>,
		effective_model: String,
		layer_config: Config,
		layer_start: std::time::Instant,
		mut total_api_time_ms: u64,
		mut total_tool_time_ms: u64,
		config: &Config,
		operation_cancelled: Arc<AtomicBool>,
	) -> Result<LayerResult> {
		// Create a mock chat session for the layer to use the unified response processing
		let mut layer_chat_session =
			self.create_layer_chat_session(messages, &effective_model, &layer_config);

		// Process the response using the same recursive logic as main sessions
		let mut current_content = initial_output;
		let mut current_exchange = initial_exchange;
		let mut current_tool_calls_param = initial_tool_calls;

		// Initialize tool processor for layer context
		let _tool_processor = crate::session::chat::ToolProcessor::new();

		// Main recursive processing loop - same as main sessions
		loop {
			// Check for cancellation at the start of each loop iteration
			if operation_cancelled.load(Ordering::SeqCst) {
				return Err(anyhow::anyhow!("Operation cancelled"));
			}

			// Check for tool calls if MCP has any servers configured for this layer
			if !self.config.mcp.server_refs.is_empty() {
				// Resolve current tool calls for this iteration (same logic as main sessions)
				let current_tool_calls =
					self.resolve_layer_tool_calls(&mut current_tool_calls_param, &current_content);

				if !current_tool_calls.is_empty() {
					// Add assistant message with tool calls preserved
					self.add_layer_assistant_message_with_tool_calls(
						&mut layer_chat_session,
						&current_content,
						&current_exchange,
					)?;

					// Execute all tool calls in parallel using the unified system
					let (tool_results, tool_time) = crate::session::chat::response::tool_execution::execute_layer_tool_calls_parallel(
						current_tool_calls,
						format!("layer_{}", self.config.name),
						&self.config,
						self.config.name.clone(),
						config,
						Some(operation_cancelled.clone()),
					).await?;

					total_tool_time_ms += tool_time;

					// Final cancellation check after all tools processed
					if operation_cancelled.load(Ordering::SeqCst) {
						return Err(anyhow::anyhow!("Operation cancelled"));
					}

					// Process tool results if any exist (same logic as main sessions)
					if !tool_results.is_empty() {
						// Use a simplified version of tool result processing for layers
						if let Some((new_content, new_exchange, new_tool_calls)) = self
							.process_layer_tool_results(
								tool_results,
								&mut layer_chat_session,
								&effective_model,
								&layer_config,
								operation_cancelled.clone(),
							)
							.await?
						{
							// Track API time from follow-up exchange
							if let Some(ref usage) = new_exchange.usage {
								if let Some(api_time) = usage.request_time_ms {
									total_api_time_ms += api_time;
								}
							}

							// Update current content for next iteration
							current_content = new_content;
							current_exchange = new_exchange;
							current_tool_calls_param = new_tool_calls;

							// Check if there are more tools to process
							if current_tool_calls_param.is_some()
								&& !current_tool_calls_param.as_ref().unwrap().is_empty()
							{
								// Continue processing the new content with tool calls
								continue;
							} else {
								// Check if there are more tool calls in the content itself
								let more_tools = crate::mcp::parse_tool_calls(&current_content);
								if !more_tools.is_empty() {
									continue;
								} else {
									// No more tool calls, break out of the loop
									break;
								}
							}
						} else {
							// No follow-up response (cancelled or error), exit
							break;
						}
					} else {
						// No tool results - check if there were more tools to execute directly
						let more_tools = crate::mcp::parse_tool_calls(&current_content);
						if !more_tools.is_empty() {
							// If there are more tool calls later in the response, continue processing
							continue;
						} else {
							// No more tool calls, exit the loop
							break;
						}
					}
				} else {
					// No tool calls in this content, break out of the loop
					break;
				}
			} else {
				// MCP not enabled for this layer, break out of the loop
				break;
			}
		}

		// Extract token usage from the final exchange (after all recursive tool processing)
		let token_usage = current_exchange.usage.clone();

		// Calculate total layer processing time
		let layer_duration = layer_start.elapsed();
		let total_time_ms = layer_duration.as_millis() as u64;

		// Return the result with time tracking using the final processed output
		Ok(LayerResult {
			output: current_content,
			exchange: current_exchange,
			token_usage,
			tool_calls: current_tool_calls_param,
			api_time_ms: total_api_time_ms,
			tool_time_ms: total_tool_time_ms,
			total_time_ms,
		})
	}

	/// Helper function to resolve current tool calls (same logic as main sessions)
	fn resolve_layer_tool_calls(
		&self,
		current_tool_calls_param: &mut Option<Vec<crate::mcp::McpToolCall>>,
		current_content: &str,
	) -> Vec<crate::mcp::McpToolCall> {
		if let Some(calls) = current_tool_calls_param.take() {
			// Use the tool calls from the API response only once
			if !calls.is_empty() {
				calls
			} else {
				crate::mcp::parse_tool_calls(current_content) // Fallback
			}
		} else {
			// For follow-up iterations, parse from content if any new tool calls exist
			crate::mcp::parse_tool_calls(current_content)
		}
	}

	/// Helper function to add assistant message with tool calls preserved (layer version)
	fn add_layer_assistant_message_with_tool_calls(
		&self,
		layer_session: &mut crate::session::chat::session::ChatSession,
		current_content: &str,
		current_exchange: &crate::session::ProviderExchange,
	) -> Result<()> {
		// Extract the original tool_calls from the exchange response
		let original_tool_calls =
			crate::session::chat::MessageHandler::extract_original_tool_calls(current_exchange);

		// Create the assistant message directly with tool_calls preserved from the exchange
		let assistant_message = Message {
			role: "assistant".to_string(),
			content: current_content.to_string(),
			timestamp: std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs(),
			cached: false,
			tool_call_id: None,
			name: None,
			tool_calls: original_tool_calls,
			images: None,
		};

		// Add the assistant message to the session
		layer_session.session.messages.push(assistant_message);

		Ok(())
	}

	/// Create a mock chat session for layer processing
	fn create_layer_chat_session(
		&self,
		messages: Vec<Message>,
		model: &str,
		_layer_config: &Config,
	) -> crate::session::chat::session::ChatSession {
		// Create a minimal session for the layer
		let mut session = crate::session::Session::new(
			format!("layer_{}", self.config.name),
			model.to_string(),
			"layer".to_string(),
		);
		session.messages = messages;

		crate::session::chat::session::ChatSession {
			session,
			model: model.to_string(),
			temperature: self.config.temperature,
			last_response: String::new(),
			estimated_cost: 0.0,
			cache_next_user_message: false,
			pending_image: None,
			spending_threshold_checkpoint: 0.0,
		}
	}

	/// Process tool results for layers (simplified version of main session logic)
	async fn process_layer_tool_results(
		&self,
		tool_results: Vec<crate::mcp::McpToolResult>,
		layer_session: &mut crate::session::chat::session::ChatSession,
		model: &str,
		layer_config: &Config,
		operation_cancelled: Arc<AtomicBool>,
	) -> Result<
		Option<(
			String,
			crate::session::ProviderExchange,
			Option<Vec<crate::mcp::McpToolCall>>,
		)>,
	> {
		// Add each tool result as a tool message
		for tool_result in &tool_results {
			let tool_content = if let Some(output) = tool_result.result.get("output") {
				if let Some(output_str) = output.as_str() {
					output_str.to_string()
				} else {
					serde_json::to_string(output).unwrap_or_default()
				}
			} else {
				serde_json::to_string(&tool_result.result).unwrap_or_default()
			};

			layer_session.session.messages.push(Message {
				role: "tool".to_string(),
				content: tool_content,
				timestamp: std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.unwrap_or_default()
					.as_secs(),
				cached: false,
				tool_call_id: Some(tool_result.tool_id.clone()),
				name: Some(tool_result.tool_name.clone()),
				tool_calls: None,
				images: None,
			});
		}

		// Check for cancellation before making another request
		if operation_cancelled.load(Ordering::SeqCst) {
			return Ok(None);
		}

		// Make follow-up API call with tool results
		match crate::session::chat_completion_with_provider(
			&layer_session.session.messages,
			model,
			self.config.temperature,
			layer_config,
		)
		.await
		{
			Ok(response) => {
				// Check if there are more tool calls to process
				let has_more_tools = if let Some(ref calls) = response.tool_calls {
					!calls.is_empty()
				} else {
					!crate::mcp::parse_tool_calls(&response.content).is_empty()
				};

				if has_more_tools {
					Ok(Some((
						response.content,
						response.exchange,
						response.tool_calls,
					)))
				} else {
					// No more tool calls, return final result
					Ok(Some((response.content, response.exchange, None)))
				}
			}
			Err(e) => {
				println!("{} {}", "Error processing layer tool results:".red(), e);
				Err(e)
			}
		}
	}
}

#[async_trait]
impl Layer for GenericLayer {
	fn name(&self) -> &str {
		&self.config.name
	}

	fn config(&self) -> &LayerConfig {
		&self.config
	}

	async fn process(
		&self,
		input: &str,
		session: &Session,
		config: &Config,
		operation_cancelled: Arc<AtomicBool>,
	) -> Result<LayerResult> {
		// Track total layer processing time
		let layer_start = std::time::Instant::now();
		let mut total_api_time_ms = 0;
		let total_tool_time_ms = 0;

		// Check if operation was cancelled
		if operation_cancelled.load(Ordering::SeqCst) {
			return Err(anyhow::anyhow!("Operation cancelled"));
		}

		// Get the effective model for this layer
		let effective_model = self.config.get_effective_model(&session.info.model);

		// Create messages for this layer
		let messages = self.create_messages(input, session, &session.info.model);

		// Create a merged config that uses this layer's MCP settings
		let layer_config = self.config.get_merged_config_for_layer(config);

		// Call the model with the layer's effective model and temperature
		let response = crate::session::chat_completion_with_provider(
			&messages,
			&effective_model,
			self.config.temperature,
			&layer_config,
		)
		.await?;

		let (output, exchange, direct_tool_calls, _finish_reason) = (
			response.content,
			response.exchange,
			response.tool_calls,
			response.finish_reason,
		);

		// Track API time from the exchange
		if let Some(ref usage) = exchange.usage {
			if let Some(api_time) = usage.request_time_ms {
				total_api_time_ms += api_time;
			}
		}

		// Check if the layer response contains tool calls and if MCP is enabled for this layer
		if !self.config.mcp.server_refs.is_empty() {
			// First try to use directly returned tool calls, then fall back to parsing if needed
			let tool_calls = if let Some(ref calls) = direct_tool_calls {
				calls
			} else {
				&crate::mcp::parse_tool_calls(&output)
			};

			// If there are tool calls, process them using this layer's MCP configuration
			if !tool_calls.is_empty() {
				// Use the unified response processing system for recursive tool call handling
				// This ensures layers have the same recursive tool call support as main sessions
				return self
					.process_recursive_tool_calls(
						output,
						exchange,
						direct_tool_calls,
						messages,
						effective_model,
						layer_config,
						layer_start,
						total_api_time_ms,
						total_tool_time_ms,
						config,
						operation_cancelled,
					)
					.await;
			}
		}

		// Extract token usage if available
		let token_usage = exchange.usage.clone();

		// Calculate total layer processing time
		let layer_duration = layer_start.elapsed();
		let total_time_ms = layer_duration.as_millis() as u64;

		// Return the result with time tracking
		Ok(LayerResult {
			output,
			exchange,
			token_usage,
			tool_calls: direct_tool_calls,
			api_time_ms: total_api_time_ms,
			tool_time_ms: total_tool_time_ms,
			total_time_ms,
		})
	}
}
