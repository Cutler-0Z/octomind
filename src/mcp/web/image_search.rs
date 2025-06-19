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

// Image search functionality

use super::super::{McpFunction, McpToolCall, McpToolResult};
use super::api_client::{
	create_api_error_result, extract_and_validate_query, make_brave_api_request,
};
use super::formatters::format_image_results;
use anyhow::{anyhow, Result};
use serde_json::json;

// Define the image_search function for the MCP protocol
pub fn get_image_search_function() -> McpFunction {
	McpFunction {
		name: "image_search".to_string(),
		description: "Search for images using Brave Search API.

Returns image search results in a token-efficient text format with titles, URLs, thumbnails, and metadata.
Requires BRAVE_API_KEY environment variable to be set.

Results format:
Each result is on a separate line with: [Rank] Title | Source URL | Image URL | Thumbnail URL

Best Practices:
- Use descriptive, visual search queries
- Use quotes for exact phrase matching: \"red sports car\"
- Be specific about what you're looking for: \"sunset over mountains\"
- Keep queries focused to get relevant image results

Examples:
- `{\"query\": \"golden retriever puppy\"}`
- `{\"query\": \"modern architecture buildings\"}`
- `{\"query\": \"vintage cars 1960s\"}`
"
		.to_string(),
		parameters: json!({
			"type": "object",
			"properties": {
				"query": {
					"type": "string",
					"description": "The search query to execute"
				},
				"count": {
					"type": "integer",
					"description": "Number of results to return (default: 50, max: 100)",
					"minimum": 1,
					"maximum": 100,
					"default": 50
				},
				"country": {
					"type": "string",
					"description": "Country code for localized results (e.g., 'US', 'GB', 'DE')",
					"default": "US"
				},
				"search_lang": {
					"type": "string",
					"description": "Language for search results (e.g., 'en', 'es', 'fr')",
					"default": "en"
				},
				"safesearch": {
					"type": "string",
					"description": "Safe search setting: 'strict' or 'off'",
					"enum": ["strict", "off"],
					"default": "strict"
				},
				"spellcheck": {
					"type": "boolean",
					"description": "Whether to enable spellcheck for the query",
					"default": true
				}
			},
			"required": ["query"]
		}),
	}
}

// Execute an image search using Brave Search API
pub async fn execute_image_search(
	call: &McpToolCall,
	_cancellation_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<McpToolResult> {
	// Extract and validate query
	let query = match extract_and_validate_query(call) {
		Ok(q) => q,
		Err(e) => {
			return Ok(create_api_error_result(
				e,
				"image",
				"image_search",
				&call.tool_id,
			))
		}
	};

	// Get API key from environment
	let api_key = std::env::var("BRAVE_API_KEY")
		.map_err(|_| anyhow!("BRAVE_API_KEY environment variable is not set"))?;

	// Extract optional parameters with defaults
	let count = call
		.parameters
		.get("count")
		.and_then(|v| v.as_u64())
		.unwrap_or(50) as u32;
	let country = call
		.parameters
		.get("country")
		.and_then(|v| v.as_str())
		.unwrap_or("US");
	let search_lang = call
		.parameters
		.get("search_lang")
		.and_then(|v| v.as_str())
		.unwrap_or("en");
	let safesearch = call
		.parameters
		.get("safesearch")
		.and_then(|v| v.as_str())
		.unwrap_or("strict");
	let spellcheck = call
		.parameters
		.get("spellcheck")
		.and_then(|v| v.as_bool())
		.unwrap_or(true);

	// Build the API URL
	let url = format!(
		"https://api.search.brave.com/res/v1/images/search?q={}&count={}&country={}&search_lang={}&safesearch={}&spellcheck={}",
		urlencoding::encode(&query),
		count,
		country,
		search_lang,
		safesearch,
		spellcheck
	);

	// Create HTTP client
	let client = reqwest::Client::new();

	// Make the API request
	let search_result = match make_brave_api_request(&client, &url, &api_key, "image").await {
		Ok(result) => result,
		Err(e) => {
			return Ok(create_api_error_result(
				e,
				"image",
				"image_search",
				&call.tool_id,
			))
		}
	};

	// Format the results
	let formatted_results = match format_image_results(&search_result, &query) {
		Ok(results) => results,
		Err(e) => {
			return Ok(create_api_error_result(
				e,
				"image",
				"image_search",
				&call.tool_id,
			))
		}
	};

	Ok(McpToolResult::success(
		"image_search".to_string(),
		call.tool_id.clone(),
		formatted_results,
	))
}
