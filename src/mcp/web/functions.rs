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

// Function definitions for the Web MCP provider

use super::super::McpFunction;
use super::search::get_web_search_function;
use serde_json::json;

pub fn get_read_html_function() -> McpFunction {
	McpFunction {
		name: "read_html".to_string(),
		description: "Convert HTML content to Markdown format from URLs or local files.

			This tool converts HTML content from web URLs or local HTML files to clean, readable Markdown.
			It's particularly useful for:
			- Reading documentation from websites in a more consumable format
			- Converting HTML files to Markdown for easier analysis
			- Processing web content without dealing with HTML tags

			The tool automatically detects whether the input is a URL or file path and:
			- Fetches content from URLs and converts to Markdown
			- Reads local HTML files and converts to Markdown
			- Handles proper conversion of HTML elements to Markdown equivalents
			- Cleans up whitespace and formatting
			- Preserves document structure and readability

			Supports multiple inputs for batch processing:
			- Single input: `{\"sources\": \"https://example.com/docs\"}`
			- Multiple inputs: `{\"sources\": [\"./docs/index.html\", \"https://example.com/api\"]}`

			Output is clean Markdown that preserves the document structure and readability."
			.to_string(),
		parameters: json!({
			"type": "object",
			"required": ["sources"],
			"properties": {
				"sources": {
					"description": "URL(s) or file path(s) to convert from HTML format to Markdown. Can be a single string or an array of strings.",
					"oneOf": [
						{
							"type": "string"
						},
						{
							"type": "array",
							"items": {
								"type": "string"
							}
						}
					]
				}
			}
		}),
	}
}

// Get all available web functions
pub fn get_all_functions() -> Vec<McpFunction> {
	vec![get_web_search_function(), get_read_html_function()]
}
