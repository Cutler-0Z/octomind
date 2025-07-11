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

// Layered response processing implementation

use super::animation::show_smart_animation;
use crate::config::Config;
use crate::session::chat::session::ChatSession;
use anyhow::Result;
use colored::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Process a response using the layered architecture
// Returns the final processed text that should be used as input for the main model
pub async fn process_layered_response(
	input: &str,
	chat_session: &mut ChatSession,
	config: &Config,
	role: &str,
	operation_cancelled: Arc<AtomicBool>,
) -> Result<String> {
	// Ensure system message is cached before processing with layers
	// This is important because system messages contain all the function definitions
	// and developer context needed for the layered processing
	let mut system_message_cached = false;

	// Check if system message is already cached
	for msg in &chat_session.session.messages {
		if msg.role == "system" && msg.cached {
			system_message_cached = true;
			break;
		}
	}

	// If system message not already cached, add a cache checkpoint
	if !system_message_cached {
		if let Ok(cached) = chat_session.session.add_cache_checkpoint(true) {
			if cached && crate::session::model_supports_caching(&chat_session.model) {
				println!(
					"{}",
					"System message has been automatically marked for caching to save tokens."
						.yellow()
				);
				// Save the session to ensure the cached status is persisted
				let _ = chat_session.save();
			}
		}
	}

	// Create a task to show loading animation with current cost
	// Use a separate flag for animation to avoid conflicts with user cancellation detection
	let animation_cancel = Arc::new(AtomicBool::new(false));
	let animation_cancel_clone = animation_cancel.clone();
	let current_cost = chat_session.session.info.total_cost;
	let animation_task = tokio::spawn(async move {
		let _ = show_smart_animation(animation_cancel_clone, current_cost).await;
	});

	// Process through the layers using the modular layered architecture
	// Each layer operates on its own session context and passes only the necessary output
	// to the next layer, ensuring proper isolation
	//
	// IMPORTANT: Each layer handles its own function calls internally with its own model
	// using the process method in processor.rs
	let layer_output: String = match crate::session::layers::process_with_layers(
		input,
		&mut chat_session.session,
		config,
		role,
		operation_cancelled.clone(),
	)
	.await
	{
		Ok(output) => output,
		Err(e) => {
			// Stop the animation using the separate animation flag
			animation_cancel.store(true, Ordering::SeqCst);
			let _ = animation_task.await;
			return Err(e);
		}
	};

	// This is important to add message as USER here for automatic processing next
	// We already added messages from output before as ASSISTANT
	chat_session.add_user_message(input)?;

	// Stop the animation using the separate animation flag
	animation_cancel.store(true, Ordering::SeqCst);
	let _ = animation_task.await;

	// Display status message for layered sessions - minimal for non-debug
	if config.get_log_level().is_debug_enabled() {
		println!("{}", "Using layered processing with model-specific caching - only supported models will use caching".bright_cyan());
	} else {
		println!("{}", "Using layered processing".bright_cyan());
	}

	// Return the processed output from layers for use in the main model conversation
	// This output already includes the results of any function calls handled by each layer
	Ok(layer_output)
}
