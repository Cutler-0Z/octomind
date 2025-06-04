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

use clap::Args;

use octomind::config::defaults::{ConfigDefaults, ConfigDefaultsExt};
use octomind::config::{Config, McpServerConfig, McpServerMode, McpServerType};
use octomind::directories;

#[derive(Args)]
pub struct ConfigArgs {
	/// Set the root-level model (provider:model format, e.g., openrouter:anthropic/claude-3.5-sonnet)
	#[arg(long)]
	pub model: Option<String>,

	/// Set API key for a provider (format: provider:key, e.g., openrouter:your-key)
	#[arg(long)]
	pub api_key: Option<String>,

	/// Set log level (none, info, debug)
	#[arg(long)]
	pub log_level: Option<String>,

	/// Set MCP providers
	#[arg(long)]
	pub mcp_providers: Option<String>,

	/// Add/configure MCP server (format: name,url=X|command=Y,args=Z)
	#[arg(long)]
	pub mcp_server: Option<String>,

	/// Set custom system prompt (or 'default' to reset to default)
	#[arg(long)]
	pub system: Option<String>,

	/// Enable markdown rendering for AI responses
	#[arg(long)]
	pub markdown_enable: Option<bool>,

	/// Set markdown theme (default, dark, light, ocean, solarized, monokai)
	#[arg(long)]
	pub markdown_theme: Option<String>,

	/// List all available markdown themes
	#[arg(long)]
	pub list_themes: bool,

	/// Show current configuration values with defaults
	#[arg(long)]
	pub show: bool,

	/// Validate configuration without making changes
	#[arg(long)]
	pub validate: bool,

	/// Reset specific field to default value (e.g., --reset-default log_level)
	#[arg(long)]
	pub reset_default: Option<String>,

	/// Show only customized (non-default) values
	#[arg(long)]
	pub show_customized: bool,

	/// Show default values for all fields
	#[arg(long)]
	pub show_defaults: bool,

	/// Upgrade config file to latest version
	#[arg(long)]
	pub upgrade: bool,
}

// Handle the configuration command
pub fn execute(args: &ConfigArgs, mut config: Config) -> Result<(), anyhow::Error> {
	// If list themes flag is set, display available themes and exit
	if args.list_themes {
		list_markdown_themes();
		return Ok(());
	}

	// If show flag is set, display current configuration with defaults and exit
	if args.show {
		show_configuration(&config)?;
		return Ok(());
	}

	// If validation flag is set, just validate and exit
	if args.validate {
		match config.validate() {
			Ok(()) => {
				println!("✅ Configuration is valid!");
				return Ok(());
			}
			Err(e) => {
				eprintln!("❌ Configuration validation failed: {}", e);
				return Err(e);
			}
		}
	}

	// If upgrade flag is set, perform manual upgrade and exit
	if args.upgrade {
		let config_path = directories::get_config_file_path()?;
		octomind::config::migrations::force_upgrade_config(&config_path)?;
		return Ok(());
	}

	// If show customized flag is set, display only non-default values and exit
	if args.show_customized {
		show_customized_configuration(&config)?;
		return Ok(());
	}

	// If show defaults flag is set, display default values and exit
	if args.show_defaults {
		show_default_values()?;
		return Ok(());
	}

	// If reset default flag is set, reset field to default and exit
	if let Some(field_name) = &args.reset_default {
		reset_field_to_default(&mut config, field_name)?;
		return Ok(());
	}

	let mut modified = false;

	// Set root-level model if specified
	if let Some(model) = &args.model {
		// Validate model format
		if !model.contains(':') {
			eprintln!("Error: Model must be in provider:model format (e.g., openrouter:anthropic/claude-3.5-sonnet)");
			return Ok(());
		}

		config.model = model.clone();
		println!("Set root-level model to {}", model);
		modified = true;
	}

	// Set API key for provider if specified
	if let Some(api_key_input) = &args.api_key {
		// Parse provider:key format
		let parts: Vec<&str> = api_key_input.splitn(2, ':').collect();
		if parts.len() != 2 {
			eprintln!("Error: API key must be in provider:key format (e.g., openrouter:your-key)");
			return Ok(());
		}

		let provider = parts[0];
		let _key = parts[1]; // Unused but needed for parsing

		// API keys are now only supported via environment variables for security
		eprintln!("❌ Error: API keys can no longer be set in config file for security reasons.");
		eprintln!("Please set the API key as an environment variable instead:");
		eprintln!(
			"  For {}: export {}_API_KEY=your-key-here",
			provider.to_uppercase(),
			provider.to_uppercase()
		);
		eprintln!("  Then restart your shell and try again.");
		return Ok(());
	}

	// Set log level if specified
	if let Some(log_level_str) = &args.log_level {
		match log_level_str.to_lowercase().as_str() {
			"none" => {
				config.log_level = octomind::config::LogLevel::None;
				println!("Set log level to None");
			}
			"info" => {
				config.log_level = octomind::config::LogLevel::Info;
				println!("Set log level to Info");
			}
			"debug" => {
				config.log_level = octomind::config::LogLevel::Debug;
				println!("Set log level to Debug");
			}
			_ => {
				eprintln!(
					"Error: Invalid log level '{}'. Valid options: none, info, debug",
					log_level_str
				);
				return Ok(());
			}
		}
		modified = true;
	}

	// Enable/disable MCP protocol - REMOVED: MCP is now controlled by role server_refs
	// MCP is enabled when roles have server_refs configured

	// Enable/disable markdown rendering
	if let Some(enable_markdown) = args.markdown_enable {
		config.enable_markdown_rendering = enable_markdown;
		println!(
			"Markdown rendering {}",
			if enable_markdown {
				"enabled"
			} else {
				"disabled"
			}
		);
		modified = true;
	}

	// Set markdown theme
	if let Some(theme) = &args.markdown_theme {
		let valid_themes = octomind::session::chat::markdown::MarkdownTheme::all_themes();
		if valid_themes.contains(&theme.as_str()) {
			config.markdown_theme = theme.clone();
			println!("Markdown theme set to '{}'", theme);
			modified = true;
		} else {
			eprintln!(
				"Error: Invalid markdown theme '{}'. Valid themes: {}",
				theme,
				valid_themes.join(", ")
			);
			return Ok(());
		}
	}

	// Update MCP server references if specified
	if let Some(providers) = &args.mcp_providers {
		let server_names: Vec<String> =
			providers.split(',').map(|s| s.trim().to_string()).collect();

		// Clear existing servers and add new ones
		config.mcp.servers.clear();
		for server_name in &server_names {
			// Create basic server config if not exists
			if !config.mcp.servers.iter().any(|s| s.name == *server_name) {
				let mut server = McpServerConfig::from_name(server_name);
				server.name = server_name.clone();
				config.mcp.servers.push(server);
			}
		}

		println!("Set MCP servers to: {}", providers);
		modified = true;
	}

	// Configure MCP server if specified
	if let Some(server_config) = &args.mcp_server {
		// Parse server config string: name,url=X|command=Y,args=Z
		let parts: Vec<&str> = server_config.split(',').collect();

		if parts.len() < 2 {
			println!("Invalid MCP server configuration format. Expected format: name,url=X|command=Y,args=Z");
		} else {
			let name = parts[0].trim().to_string();

			// Create a new server config
			let mut server = McpServerConfig {
				name: name.clone(),
				server_type: McpServerType::External, // Default to external type
				url: None,
				command: None,
				args: Vec::new(),
				auth_token: None,
				mode: McpServerMode::Http, // Default to HTTP mode
				tools: Vec::new(),
				timeout_seconds: 30, // Default timeout
				builtin: false,      // User-created servers are not builtin
			};

			// Process remaining parts
			for part in &parts[1..] {
				let kv: Vec<&str> = part.split('=').collect();
				if kv.len() == 2 {
					let key = kv[0].trim();
					let value = kv[1].trim();

					match key {
						"url" => {
							server.url = Some(value.to_string());
						}
						"command" => {
							server.command = Some(value.to_string());
						}
						"args" => {
							server.args = value
								.split(' ')
								.map(|s| s.trim().to_string())
								.filter(|s| !s.is_empty())
								.collect();
						}
						"token" | "auth_token" => {
							server.auth_token = Some(value.to_string());
						}
						"mode" => match value.to_lowercase().as_str() {
							"http" => server.mode = McpServerMode::Http,
							"stdin" => server.mode = McpServerMode::Stdin,
							_ => println!("Unknown server mode: {}, defaulting to HTTP", value),
						},
						"timeout" | "timeout_seconds" => {
							if let Ok(timeout) = value.parse::<u64>() {
								server.timeout_seconds = timeout;
							} else {
								println!("Invalid timeout value: {}, using default", value);
							}
						}
						_ => {
							println!("Unknown server config key: {}", key);
						}
					}
				}
			}

			// Validate the server config
			match server.server_type {
				McpServerType::External => {
					if server.url.is_none() && server.command.is_none() {
						println!("Error: Either url or command must be specified for external MCP server");
						return Ok(());
					}
				}
				_ => {
					// Built-in servers are always valid
				}
			}

			// Enable MCP if not already enabled - REMOVED: MCP now controlled by server_refs
			// The presence of servers in the registry doesn't automatically enable MCP

			// Add the new server to registry
			// Remove existing server with same name first
			config.mcp.servers.retain(|s| s.name != name);
			// Set the name and add the server
			server.name = name.clone();
			config.mcp.servers.push(server);

			println!("Added/updated MCP server: {}", name);
			modified = true;
		}
	}

	// Update system prompt if specified
	if let Some(system_prompt) = &args.system {
		if system_prompt.to_lowercase() == "default" {
			// Reset to default
			config.system = None;
			println!("Reset system prompt to default");
		} else {
			// Set custom prompt
			config.system = Some(system_prompt.clone());
			println!("Set custom system prompt");
		}
		modified = true;
	}

	// If no modifications were made, create a default config
	if !modified {
		// Check if config file already exists
		let config_path = directories::get_config_file_path()?;

		if config_path.exists() {
			println!(
				"Configuration file already exists at: {}",
				config_path.display()
			);
			println!("No changes were made to the configuration.");
		} else {
			let config_path = Config::create_default_config()?;
			println!(
				"Created default configuration file at: {}",
				config_path.display()
			);
		}
	} else {
		// Save the updated configuration
		if let Err(e) = config.save() {
			eprintln!("Error saving configuration: {}", e);
			return Err(e);
		}
		println!("Configuration saved successfully");
	}

	// Show current configuration
	println!("\nCurrent configuration:");

	// Show root-level model
	println!("Root model: {}", config.get_effective_model());

	// Show provider API keys (from environment variables only)
	println!("Provider API keys (from environment variables):");
	show_env_api_key_status("  OpenRouter", "OPENROUTER_API_KEY");
	show_env_api_key_status("  OpenAI", "OPENAI_API_KEY");
	show_env_api_key_status("  Anthropic", "ANTHROPIC_API_KEY");
	show_env_api_key_status("  Google", "GOOGLE_APPLICATION_CREDENTIALS");
	show_env_api_key_status("  Amazon", "AWS_ACCESS_KEY_ID");
	show_env_api_key_status("  Cloudflare", "CLOUDFLARE_API_TOKEN");

	// Show role configurations (models now use system-wide setting)
	println!("Role configurations:");

	// Show MCP status using the new structure
	// MCP is enabled per-role based on server_refs, not a global flag
	let dev_mcp_enabled = !config.developer.mcp.server_refs.is_empty();
	let ass_mcp_enabled = !config.assistant.mcp.server_refs.is_empty();

	println!("MCP status:");
	println!(
		"  Developer role: {}",
		if dev_mcp_enabled {
			"enabled"
		} else {
			"disabled"
		}
	);
	println!(
		"  Assistant role: {}",
		if ass_mcp_enabled {
			"enabled"
		} else {
			"disabled"
		}
	);

	// Show MCP servers from global config
	if !config.mcp.servers.is_empty() || dev_mcp_enabled || ass_mcp_enabled {
		if !config.mcp.servers.is_empty() {
			println!("MCP servers:");
			for server in &config.mcp.servers {
				let name = &server.name;
				// Note: enabled status is now determined by role server_refs, not individual server config
				// Here we just show what's available in the registry

				// Auto-detect server type for display
				let effective_type = match name.as_str() {
					"developer" => McpServerType::Developer,
					"filesystem" => McpServerType::Filesystem,
					_ => McpServerType::External,
				};

				match effective_type {
					McpServerType::Developer => {
						println!("  - {} (built-in developer tools) - available", name)
					}
					McpServerType::Filesystem => {
						println!("  - {} (built-in filesystem tools) - available", name)
					}
					McpServerType::External => {
						if name == "octocode" {
							// Check if octocode binary is available
							use std::process::Command;
							let available = match Command::new("octocode").arg("--version").output()
							{
								Ok(output) => output.status.success(),
								Err(_) => false,
							};

							if available {
								println!("  - {} (codebase analysis) - available ✓", name);
							} else {
								println!(
									"  - {} (codebase analysis) - binary not found in PATH",
									name
								);
							}
						} else if let Some(url) = &server.url {
							println!("  - {} (HTTP: {}) - available", name, url);
						} else if let Some(command) = &server.command {
							println!("  - {} (Command: {}) - available", name, command);
						} else {
							println!(
								"  - {} (external, not configured) - needs configuration",
								name
							);
						}
					}
				}
			}
		} else {
			println!("MCP servers: None configured");
		}
	}

	println!("Log level: {:?}", config.log_level);
	println!(
		"Markdown rendering: {}",
		if config.enable_markdown_rendering {
			"enabled"
		} else {
			"disabled"
		}
	);
	println!("Markdown theme: {}", config.markdown_theme);

	// Show system prompt status
	if config.system.is_some() {
		println!("System prompt: Custom");
	} else {
		println!("System prompt: Default");
	}

	Ok(())
}

/// Display available markdown themes with descriptions
fn list_markdown_themes() {
	println!("🎨 Available Markdown Themes\n");

	let themes = vec![
		(
			"default",
			"Improved default theme with gold headers and enhanced contrast",
			"Most terminal setups",
		),
		(
			"dark",
			"Optimized for dark terminals with bright, vibrant colors",
			"Dark terminal backgrounds",
		),
		(
			"light",
			"Perfect for light terminal backgrounds with darker colors",
			"Light terminal backgrounds",
		),
		(
			"ocean",
			"Beautiful blue-green palette inspired by ocean themes",
			"Users who prefer calm, aquatic colors",
		),
		(
			"solarized",
			"Based on the popular Solarized color scheme",
			"Fans of the classic Solarized palette",
		),
		(
			"monokai",
			"Inspired by the popular Monokai syntax highlighting theme",
			"Users familiar with Monokai from code editors",
		),
	];

	for (name, description, best_for) in themes {
		println!("📝 {}", name.to_uppercase());
		println!("   Description: {}", description);
		println!("   Best for:    {}", best_for);
		println!("   Usage:       octomind config --markdown-theme {}", name);
		println!();
	}

	println!("💡 Tips:");
	println!("   • Themes work in sessions, ask command, and multimode");
	println!("   • Enable markdown rendering: octomind config --markdown-enable true");
	println!("   • View current theme: octomind config --show");
}

/// Display comprehensive configuration information with defaults
fn show_configuration(config: &Config) -> Result<(), anyhow::Error> {
	println!("🔧 Octomind Configuration\n");

	// Configuration file location
	let config_path = directories::get_config_file_path()?;
	if config_path.exists() {
		println!("📁 Config file: {}", config_path.display());
	} else {
		println!(
			"📁 Config file: {} (not created yet)",
			config_path.display()
		);
	}
	println!();

	// Root-level configuration
	println!("🌍 System-wide Settings");
	println!(
		"  Model (root):              {}",
		if config.model.is_empty() || config.model == "openrouter:anthropic/claude-3.5-haiku" {
			format!("{} (default)", config.get_effective_model())
		} else {
			config.model.clone()
		}
	);
	println!("  Log level:                 {:?}", config.log_level);
	println!(
		"  Markdown rendering:        {}",
		if config.enable_markdown_rendering {
			"enabled"
		} else {
			"disabled"
		}
	);
	println!("  Markdown theme:            {}", config.markdown_theme);
	println!(
		"  MCP response warning:      {} tokens",
		config.mcp_response_warning_threshold
	);
	println!(
		"  Max request tokens:        {} tokens",
		config.max_request_tokens_threshold
	);
	println!(
		"  Auto-truncation:           {}",
		if config.enable_auto_truncation {
			"enabled"
		} else {
			"disabled"
		}
	);
	println!(
		"  Cache threshold:           {} tokens",
		config.cache_tokens_threshold
	);
	println!(
		"  Cache timeout:             {} seconds",
		config.cache_timeout_seconds
	);
	println!();

	// Provider API keys (from environment variables only)
	println!("🔑 Provider API Keys (from environment variables)");
	show_env_api_key_status("OpenRouter", "OPENROUTER_API_KEY");
	show_env_api_key_status("OpenAI", "OPENAI_API_KEY");
	show_env_api_key_status("Anthropic", "ANTHROPIC_API_KEY");
	show_env_api_key_status("Google", "GOOGLE_APPLICATION_CREDENTIALS");
	show_env_api_key_status("Amazon", "AWS_ACCESS_KEY_ID");
	show_env_api_key_status("Cloudflare", "CLOUDFLARE_API_TOKEN");
	println!();

	// Role configurations
	println!("👤 Role Configurations");

	// Developer role
	println!("  Developer Role:");
	let (dev_config, dev_mcp, dev_layers, _dev_commands, dev_system) =
		config.get_mode_config("developer");
	println!(
		"    Model:           {} (system-wide)",
		config.get_effective_model()
	);
	println!("    Layers enabled:  {}", dev_config.enable_layers);
	if let Some(_system) = dev_system {
		println!("    System prompt:   Custom");
	} else {
		println!("    System prompt:   Default");
	}

	// Assistant role
	println!("  Assistant Role:");
	let (ass_config, ass_mcp, _ass_layers, _ass_commands, ass_system) =
		config.get_mode_config("assistant");
	println!(
		"    Model:           {} (system-wide)",
		config.get_effective_model()
	);
	println!("    Layers enabled:  {}", ass_config.enable_layers);
	if let Some(_system) = ass_system {
		println!("    System prompt:   Custom");
	} else {
		println!("    System prompt:   Default");
	}
	println!();

	// MCP Configuration
	println!("🔧 MCP (Model Context Protocol) Configuration");

	// Global MCP
	println!("  Global MCP:");
	println!(
		"    Registry:        {} servers configured",
		config.mcp.servers.len()
	);
	if !config.mcp.servers.is_empty() {
		show_mcp_servers(&config.mcp.servers);
	}

	// Developer role MCP
	println!("  Developer Role MCP:");
	println!(
		"    Server refs:     {}",
		if dev_mcp.server_refs.is_empty() {
			"None (MCP disabled)".to_string()
		} else {
			dev_mcp.server_refs.join(", ")
		}
	);

	// Assistant role MCP
	println!("  Assistant Role MCP:");
	println!(
		"    Server refs:     {}",
		if ass_mcp.server_refs.is_empty() {
			"None (MCP disabled)".to_string()
		} else {
			ass_mcp.server_refs.join(", ")
		}
	);
	println!();

	// Layer configurations
	if dev_config.enable_layers || ass_config.enable_layers {
		println!("📚 Layer Configurations");

		if let Some(layers) = dev_layers {
			println!("  Developer Role Layers: {} configured", layers.len());
			for layer in layers {
				// All configured layers are considered enabled (no more 'enabled' field)
				println!("    ✅ {} (temp: {:.1})", layer.name, layer.temperature);
			}
		}

		if let Some(layers) = &config.layers {
			println!("  Global Layers: {} configured", layers.len());
			for layer in layers {
				// All configured layers are considered enabled (no more 'enabled' field)
				println!("    ✅ {} (temp: {:.1})", layer.name, layer.temperature);
			}
		}
		println!();
	}

	Ok(())
}

/// Show the status of an API key with environment variable fallback
fn show_env_api_key_status(provider: &str, env_var: &str) {
	if std::env::var(env_var).is_ok() {
		println!(
			"{:<15} ✅ Set via {} environment variable",
			provider, env_var
		);
	} else {
		println!("{:<15} ❌ Not set (export {}=your-key)", provider, env_var);
	}
}

/// Display MCP server configurations
fn show_mcp_servers(servers: &Vec<McpServerConfig>) {
	if servers.is_empty() {
		println!("    Servers:         None configured");
		return;
	}

	println!("    Servers:");
	for server in servers {
		let name = &server.name;
		// Note: Individual servers no longer have enabled flag - determined by role server_refs

		// Auto-detect server type for display
		let effective_type = match name.as_str() {
			"developer" => McpServerType::Developer,
			"filesystem" => McpServerType::Filesystem,
			_ => McpServerType::External,
		};

		match effective_type {
			McpServerType::Developer => {
				println!("      📦 {} (built-in developer tools)", name);
			}
			McpServerType::Filesystem => {
				println!("      📂 {} (built-in filesystem tools)", name);
			}
			McpServerType::External => {
				if name == "octocode" {
					// Check if octocode binary is available
					use std::process::Command;
					let available = match Command::new("octocode").arg("--version").output() {
						Ok(output) => output.status.success(),
						Err(_) => false,
					};

					if available {
						println!("      🔍 {} (codebase analysis) ✓", name);
					} else {
						println!("      🔍 {} (binary not found in PATH)", name);
					}
				} else if let Some(url) = &server.url {
					println!("      🌐 {} (HTTP: {})", name, url);
				} else if let Some(command) = &server.command {
					println!("      ⚙️  {} (Command: {})", name, command);
				} else {
					println!("      ❓ {} (external, not configured)", name);
				}
			}
		}

		// Show additional server details if configured
		if server.timeout_seconds != 30 {
			println!("        Timeout: {} seconds", server.timeout_seconds);
		}
		if !server.tools.is_empty() {
			println!("        Tools: {}", server.tools.join(", "));
		}
	}
}

/// Mask an API key for display purposes
/// Show only customized (non-default) configuration values
fn show_customized_configuration(config: &Config) -> Result<(), anyhow::Error> {
	println!("🔧 Customized Configuration Values\n");

	let customized_fields = config.get_customized_fields();

	if customized_fields.is_empty() {
		println!("✅ All configuration values are using defaults.");
		println!("   Use 'octomind config --show-defaults' to see what the defaults are.");
		return Ok(());
	}

	println!("📝 The following fields have been customized from their defaults:\n");

	for field in &customized_fields {
		let current_value = get_current_field_value(config, field);
		let default_value = config
			.get_default_value_string(field)
			.unwrap_or("N/A".to_string());

		println!("  🔹 {}", field);
		println!("     Current: {}", current_value);
		println!("     Default: {}", default_value);
		println!();
	}

	println!("💡 Tips:");
	println!("   • Reset a field to default: octomind config --reset-default <field_name>");
	println!("   • View all defaults: octomind config --show-defaults");
	println!("   • View full config: octomind config --show");

	Ok(())
}

/// Show default values for all configuration fields
fn show_default_values() -> Result<(), anyhow::Error> {
	println!("🎯 Default Configuration Values\n");

	println!("These are the built-in default values for all configuration options:");
	println!("You can customize any of these in your config file or via command line.\n");

	// Root-level defaults
	println!("🌍 System-wide Defaults:");
	println!(
		"  log_level:                     {:?}",
		ConfigDefaults::DEFAULT_LOG_LEVEL
	);
	println!(
		"  model:                         {}",
		ConfigDefaults::DEFAULT_MODEL
	);
	println!(
		"  mcp_response_warning_threshold: {}",
		ConfigDefaults::DEFAULT_MCP_RESPONSE_WARNING_THRESHOLD
	);
	println!(
		"  max_request_tokens_threshold:  {}",
		ConfigDefaults::DEFAULT_MAX_REQUEST_TOKENS_THRESHOLD
	);
	println!(
		"  enable_auto_truncation:        {}",
		ConfigDefaults::DEFAULT_ENABLE_AUTO_TRUNCATION
	);
	println!(
		"  cache_tokens_threshold:        {}",
		ConfigDefaults::DEFAULT_CACHE_TOKENS_THRESHOLD
	);
	println!(
		"  cache_timeout_seconds:         {}",
		ConfigDefaults::DEFAULT_CACHE_TIMEOUT_SECONDS
	);
	println!(
		"  enable_markdown_rendering:     {}",
		ConfigDefaults::DEFAULT_ENABLE_MARKDOWN_RENDERING
	);
	println!(
		"  markdown_theme:                {}",
		ConfigDefaults::DEFAULT_MARKDOWN_THEME
	);
	println!(
		"  max_session_spending_threshold: {}",
		ConfigDefaults::DEFAULT_MAX_SESSION_SPENDING_THRESHOLD
	);
	println!();

	// Role defaults
	println!("👤 Role Defaults:");
	println!(
		"  developer.enable_layers:       {}",
		ConfigDefaults::DEFAULT_ENABLE_LAYERS
	);
	println!(
		"  developer.mcp.server_refs:     [{}]",
		ConfigDefaults::DEFAULT_DEVELOPER_SERVER_REFS.join(", ")
	);
	println!(
		"  assistant.enable_layers:       {}",
		ConfigDefaults::DEFAULT_ENABLE_LAYERS
	);
	println!(
		"  assistant.mcp.server_refs:     [{}]",
		ConfigDefaults::DEFAULT_ASSISTANT_SERVER_REFS.join(", ")
	);
	println!();

	// MCP defaults
	println!("🔧 MCP Defaults:");
	println!(
		"  mcp_server_timeout:            {} seconds",
		ConfigDefaults::DEFAULT_MCP_SERVER_TIMEOUT
	);
	println!();

	// Optional fields (None by default)
	println!("📝 Optional Fields (None by default):");
	println!("  developer.system:              None (uses built-in prompt)");
	println!("  assistant.system:              None (uses built-in prompt)");
	println!("  layers:                        None (no custom layers)");
	println!("  commands:                      None (no custom commands)");
	println!("  system:                        None (uses role-specific prompts)");
	println!();

	println!("💡 Tips:");
	println!("   • View your current config: octomind config --show");
	println!("   • View only customized values: octomind config --show-customized");
	println!("   • Reset a field to default: octomind config --reset-default <field_name>");

	Ok(())
}

/// Reset a specific field to its default value
fn reset_field_to_default(config: &mut Config, field_name: &str) -> Result<(), anyhow::Error> {
	// Get the current value for display
	let current_value = get_current_field_value(config, field_name);
	let default_value = config.get_default_value_string(field_name);

	if let Some(default_val) = &default_value {
		println!("🔄 Resetting '{}' to default value", field_name);
		println!("   Current: {}", current_value);
		println!("   Default: {}", default_val);

		// Reset the field
		config.reset_to_default(field_name)?;

		// Save the configuration
		config.save()?;

		println!("✅ Field '{}' has been reset to default value", field_name);
	} else {
		return Err(anyhow::anyhow!(
			"Unknown field '{}'. Use 'octomind config --show-defaults' to see available fields.",
			field_name
		));
	}

	Ok(())
}

/// Get the current value of a field as a string for display
fn get_current_field_value(config: &Config, field_name: &str) -> String {
	match field_name {
		"log_level" => format!("{:?}", config.log_level),
		"model" => config.model.clone(),
		"mcp_response_warning_threshold" => config.mcp_response_warning_threshold.to_string(),
		"max_request_tokens_threshold" => config.max_request_tokens_threshold.to_string(),
		"enable_auto_truncation" => config.enable_auto_truncation.to_string(),
		"cache_tokens_threshold" => config.cache_tokens_threshold.to_string(),
		"cache_timeout_seconds" => config.cache_timeout_seconds.to_string(),
		"enable_markdown_rendering" => config.enable_markdown_rendering.to_string(),
		"markdown_theme" => config.markdown_theme.clone(),
		"max_session_spending_threshold" => config.max_session_spending_threshold.to_string(),
		"developer.enable_layers" => config.developer.config.enable_layers.to_string(),
		"assistant.enable_layers" => config.assistant.config.enable_layers.to_string(),
		"developer.mcp.server_refs" => format!("[{}]", config.developer.mcp.server_refs.join(", ")),
		"assistant.mcp.server_refs" => format!("[{}]", config.assistant.mcp.server_refs.join(", ")),
		"developer.system" => config
			.developer
			.config
			.system
			.as_ref()
			.unwrap_or(&"None".to_string())
			.clone(),
		"assistant.system" => config
			.assistant
			.config
			.system
			.as_ref()
			.unwrap_or(&"None".to_string())
			.clone(),
		"layers" => {
			if config.layers.is_some() {
				format!(
					"{} layers configured",
					config.layers.as_ref().unwrap().len()
				)
			} else {
				"None".to_string()
			}
		}
		"commands" => {
			if config.commands.is_some() {
				format!(
					"{} commands configured",
					config.commands.as_ref().unwrap().len()
				)
			} else {
				"None".to_string()
			}
		}
		"system" => config
			.system
			.as_ref()
			.unwrap_or(&"None".to_string())
			.clone(),
		_ => "Unknown field".to_string(),
	}
}
