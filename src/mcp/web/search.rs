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

// Web search functionality using Brave Search API

use super::super::{McpFunction, McpToolCall, McpToolResult};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

// Define the web_search function for the MCP protocol
pub fn get_web_search_function() -> McpFunction {
	McpFunction {
		name: "web_search".to_string(),
		description: "Search the web using Brave Search API.

Returns search results in a token-efficient text format with titles, URLs, and descriptions.
Requires BRAVE_API_KEY environment variable to be set.

Results format:
Each result is on a separate line with: [Rank] Title | URL | Description

Best Practices:
- Use specific, targeted search queries
- Use quotes for exact phrase matching: \"exact phrase\"
- Use site: operator for specific domains: site:github.com
- Use - operator to exclude terms: python -django
- Keep queries focused to get relevant results

Examples:
- `{\"query\": \"rust web framework\"}`
- `{\"query\": \"\\\"machine learning\\\" tutorial\"}`
- `{\"query\": \"site:stackoverflow.com async rust\"}`
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
					"description": "Number of results to return (default: 20, max: 20)",
					"minimum": 1,
					"maximum": 20,
					"default": 20
				},
				"offset": {
					"type": "integer",
					"description": "Number of results to skip for pagination (default: 0, max: 9)",
					"minimum": 0,
					"maximum": 9,
					"default": 0
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
				"ui_lang": {
					"type": "string",
					"description": "Language for UI elements (e.g., 'en-US', 'es-ES', 'fr-FR')",
					"default": "en-US"
				},
				"safesearch": {
					"type": "string",
					"description": "Safe search setting: 'strict', 'moderate', or 'off'",
					"enum": ["strict", "moderate", "off"],
					"default": "moderate"
				},
				"freshness": {
					"type": "string",
					"description": "Time filter for results: 'pd' (past day), 'pw' (past week), 'pm' (past month), 'py' (past year)",
					"enum": ["pd", "pw", "pm", "py"]
				}
			},
			"required": ["query"]
		}),
	}
}

// Execute a web search using Brave Search API
pub async fn execute_web_search(
	call: &McpToolCall,
	_cancellation_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<McpToolResult> {
	// Check for BRAVE_API_KEY environment variable
	let api_key =
		std::env::var("BRAVE_API_KEY").map_err(|_| {
			anyhow!("BRAVE_API_KEY environment variable is not set. Please set your Brave Search API key.")
		})?;

	// Extract query parameter
	let query = match call.parameters.get("query") {
		Some(Value::String(q)) => q.clone(),
		_ => return Err(anyhow!("Missing or invalid 'query' parameter")),
	};

	// Validate query according to Brave API limits
	if query.is_empty() {
		return Err(anyhow!("Query cannot be empty"));
	}
	if query.len() > 400 {
		return Err(anyhow!("Query too long: maximum 400 characters allowed"));
	}
	let word_count = query.split_whitespace().count();
	if word_count > 50 {
		return Err(anyhow!(
			"Query has too many words: maximum 50 words allowed"
		));
	}

	// Extract optional parameters with defaults
	let count = call
		.parameters
		.get("count")
		.and_then(|v| v.as_i64())
		.unwrap_or(20)
		.clamp(1, 20) as u32;

	let offset = call
		.parameters
		.get("offset")
		.and_then(|v| v.as_i64())
		.unwrap_or(0)
		.clamp(0, 9) as u32;

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

	let ui_lang = call
		.parameters
		.get("ui_lang")
		.and_then(|v| v.as_str())
		.unwrap_or("en-US");

	let safesearch = call
		.parameters
		.get("safesearch")
		.and_then(|v| v.as_str())
		.unwrap_or("moderate");

	let freshness = call.parameters.get("freshness").and_then(|v| v.as_str());

	// Build the API request using reqwest's query parameter builder
	let client = reqwest::Client::new();

	// Create string representations for numeric parameters
	let count_str = count.to_string();
	let offset_str = offset.to_string();

	// Start with minimal required parameters
	let mut query_params = vec![("q", query.as_str())];

	// Add optional parameters only if they differ from defaults
	if count != 20 {
		query_params.push(("count", count_str.as_str()));
	}
	if offset != 0 {
		query_params.push(("offset", offset_str.as_str()));
	}
	if country != "US" {
		query_params.push(("country", country));
	}
	if search_lang != "en" {
		query_params.push(("search_lang", search_lang));
	}
	if ui_lang != "en-US" {
		query_params.push(("ui_lang", ui_lang));
	}
	if safesearch != "moderate" {
		query_params.push(("safesearch", safesearch));
	}

	let mut request = client
		.get("https://api.search.brave.com/res/v1/web/search")
		.query(&query_params);

	// Add optional freshness parameter
	if let Some(f) = freshness {
		request = request.query(&[("freshness", f)]);
	}

	// Make the API request
	crate::log_debug!("Making Brave Search API request for query: '{}'", query);
	crate::log_debug!("Request parameters: count={}, offset={}, country={}, search_lang={}, ui_lang={}, safesearch={}",
		count, offset, country, search_lang, ui_lang, safesearch);

	let response = request
		.header("Accept", "application/json")
		.header("Accept-Encoding", "gzip")
		.header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
		.header("X-Subscription-Token", &api_key)
		.send()
		.await
		.map_err(|e| anyhow!("Failed to make request to Brave Search API: {}", e))?;

	crate::log_debug!("Brave Search API response status: {}", response.status());
	crate::log_debug!(
		"Brave Search API response headers: {:?}",
		response.headers()
	);

	// Check if request was successful
	if !response.status().is_success() {
		let status = response.status();
		let error_text = response.text().await.unwrap_or_default();

		// Provide more specific error messages for common issues
		let error_msg = match status.as_u16() {
			401 => {
				"Invalid or missing API key. Please check your BRAVE_API_KEY environment variable."
					.to_string()
			}
			422 => format!("Invalid request parameters. API response: {}", error_text),
			429 => "Rate limit exceeded. Please wait before making more requests.".to_string(),
			403 => "Access forbidden. Please check your subscription plan and API key permissions."
				.to_string(),
			_ => format!(
				"Brave Search API request failed with status {}: {}",
				status, error_text
			),
		};

		return Err(anyhow!("{}", error_msg));
	}

	// Get the response text first for better error handling
	let response_text = response
		.text()
		.await
		.map_err(|e| anyhow!("Failed to read Brave Search API response: {}", e))?;

	crate::log_debug!("Brave Search API response: {}", response_text);

	// Parse the response
	let search_result: Value = serde_json::from_str(&response_text).map_err(|e| {
		anyhow!(
			"Failed to parse Brave Search API response as JSON: {}. Response was: {}",
			e,
			response_text
		)
	})?;

	// Extract and format the results as simple text
	let formatted_text = match format_search_results(&search_result, &query) {
		Ok(text) => text,
		Err(e) => {
			crate::log_debug!("Failed to format search results: {}", e);
			format!(
				"Search failed: {}\n\nRaw response available in debug logs.",
				e
			)
		}
	};

	// Use MCP success format with simple text content
	Ok(super::super::McpToolResult::success(
		"web_search".to_string(),
		call.tool_id.clone(),
		formatted_text,
	))
}

// Format search results as simple, token-efficient text
fn format_search_results(search_result: &Value, query: &str) -> Result<String> {
	// Debug: log the structure we received
	crate::log_debug!(
		"Parsing search result structure. Available keys: {:?}",
		search_result
			.as_object()
			.map(|o| o.keys().collect::<Vec<_>>())
	);

	// Check if we have web results
	let web_section = search_result.get("web");
	if web_section.is_none() {
		crate::log_debug!("No 'web' section found in response");
		return Ok(format!("No web results found for query: '{}'", query));
	}

	let web_results = web_section
		.and_then(|w| w.get("results"))
		.and_then(|r| r.as_array());

	if web_results.is_none() {
		crate::log_debug!("No 'results' array found in web section");
		return Ok(format!("No search results found for query: '{}'", query));
	}

	let web_results = web_results.unwrap();

	if web_results.is_empty() {
		return Ok(format!("No search results found for query: '{}'", query));
	}

	// Get total count for header
	let total_count = search_result
		.get("web")
		.and_then(|w| w.get("totalCount"))
		.and_then(|t| t.as_i64())
		.unwrap_or(0);

	let mut result_text = format!(
		"Search results for '{}' ({} total results):\n\n",
		query, total_count
	);

	for (index, result) in web_results.iter().enumerate() {
		let title = result
			.get("title")
			.and_then(|t| t.as_str())
			.unwrap_or("No title");

		let url = result
			.get("url")
			.and_then(|u| u.as_str())
			.unwrap_or("No URL");

		let description = result
			.get("description")
			.and_then(|d| d.as_str())
			.unwrap_or("No description");

		// Format: [Rank] Title | URL | Description
		result_text.push_str(&format!(
			"[{}] {} | {} | {}\n",
			index + 1,
			title,
			url,
			description
		));
	}

	Ok(result_text)
}
