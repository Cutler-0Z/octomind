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

use anyhow::{Context, Result};
use std::fs;

use super::Config;

impl Config {
	fn initialize_config(&mut self) {}

	pub fn ensure_octomind_dir() -> Result<std::path::PathBuf> {
		// Use the system-wide directory
		crate::directories::get_octomind_data_dir()
	}

	/// Copy the default configuration template when no config exists
	pub fn copy_default_config_template(config_path: &std::path::Path) -> Result<()> {
		// Default config template embedded in binary
		const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../config-templates/default.toml");

		// Ensure the parent directory exists
		if let Some(parent) = config_path.parent() {
			fs::create_dir_all(parent).context(format!(
				"Failed to create config directory: {}",
				parent.display()
			))?;
		}

		// Write the default template
		fs::write(config_path, DEFAULT_CONFIG_TEMPLATE).context(format!(
			"Failed to write default config template to {}",
			config_path.display()
		))?;

		println!("Created default configuration at {}", config_path.display());
		println!("Please edit the configuration file to set your API keys and preferences.");

		Ok(())
	}

	/// Create default config at the standard location (public version for commands)
	pub fn create_default_config() -> Result<std::path::PathBuf> {
		let config_path = crate::directories::get_config_file_path()?;

		if !config_path.exists() {
			Self::copy_default_config_template(&config_path)?;
		}

		Ok(config_path)
	}

	/// Inject default configuration directly from embedded TOML template
	fn inject_default_config() -> Result<Self> {
		// Use the existing embedded template, but parse directly into memory
		const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../config-templates/default.toml");

		let mut config: Config = toml::from_str(DEFAULT_CONFIG_TEMPLATE)
			.context("Failed to parse default configuration template")?;

		// Build role map from roles array
		config.build_role_map();

		Ok(config)
	}

	/// Load configuration from the system-wide config file with strict validation
	pub fn load() -> Result<Self> {
		// Use the new system-wide config file path
		let config_path = crate::directories::get_config_file_path()?;

		if !config_path.exists() {
			// Inject default configuration
			let default_config = Self::inject_default_config()?;

			// Still write to file for future edits
			default_config.save_to_path(&config_path)?;
		}

		// Check for automatic config upgrades
		super::migrations::check_and_upgrade_config(&config_path)
			.context("Failed to check/upgrade config version")?;

		let config_str = fs::read_to_string(&config_path).context(format!(
			"Failed to read config from {}",
			config_path.display()
		))?;

		let mut config: Config = toml::from_str(&config_str).context(
			"Failed to parse TOML configuration. All required fields must be present in strict mode."
		)?;

		// Store the config path for future saves
		config.config_path = Some(config_path);

		// Initialize the configuration
		config.initialize_config();

		// Build role map from roles array
		config.build_role_map();

		// REMOVED: API key population from environment variables
		// API keys are now read directly from ENV when needed by providers

		// STRICT validation - fail if configuration is invalid
		config.validate()?;

		Ok(config)
	}

	/// REMOVED: No more default_with_env - config must be complete and explicit
	/// All defaults are now in the template file
	///
	/// Save configuration to file
	pub fn save(&self) -> Result<()> {
		// Validate before saving
		self.validate()?;

		// Use the stored config path, or fallback to system-wide default
		let config_path = if let Some(path) = &self.config_path {
			path.clone()
		} else {
			crate::directories::get_config_file_path()?
		};

		// Ensure the parent directory exists
		if let Some(parent) = config_path.parent() {
			fs::create_dir_all(parent).context(format!(
				"Failed to create config directory: {}",
				parent.display()
			))?;
		}

		// Serialize to TOML
		let config_str =
			toml::to_string_pretty(self).context("Failed to serialize configuration to TOML")?;

		// Write to file
		fs::write(&config_path, config_str).context(format!(
			"Failed to write config to {}",
			config_path.display()
		))?;

		println!("Configuration saved to {}", config_path.display());
		Ok(())
	}

	/// Load configuration from a specific file path
	pub fn load_from_path(path: &std::path::Path) -> Result<Self> {
		let config_str = fs::read_to_string(path)
			.context(format!("Failed to read config from {}", path.display()))?;
		let mut config: Config =
			toml::from_str(&config_str).context("Failed to parse TOML configuration")?;

		// Store the config path for future saves
		config.config_path = Some(path.to_path_buf());

		// Initialize the configuration
		config.initialize_config();

		// Build role map from roles array
		config.build_role_map();

		// Validate the configuration
		config.validate()?;

		Ok(config)
	}

	/// Save configuration to a specific file path
	pub fn save_to_path(&self, path: &std::path::Path) -> Result<()> {
		// Validate before saving
		self.validate()?;

		// Ensure the parent directory exists
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).context(format!(
				"Failed to create config directory: {}",
				parent.display()
			))?;
		}

		// Serialize to TOML
		let config_str =
			toml::to_string_pretty(self).context("Failed to serialize configuration to TOML")?;

		// Write to file
		fs::write(path, config_str)
			.context(format!("Failed to write config to {}", path.display()))?;

		println!("Configuration saved to {}", path.display());
		Ok(())
	}

	/// Create a clean copy of the config for saving (removes runtime-only fields)
	pub fn create_clean_copy_for_saving(&self) -> Self {
		// Only remove servers that are marked as runtime-only or temporary
		// (Currently there are no runtime-only servers, so we keep all servers)

		// Keep the MCP section to show what's actually available

		self.clone()
	}

	/// Update configuration with a closure and save
	pub fn update_and_save<F>(&mut self, updater: F) -> Result<()>
	where
		F: FnOnce(&mut Self),
	{
		// Validate before saving
		self.validate()?;

		// Use the stored config path, or fallback to system-wide default
		let config_path = if let Some(path) = &self.config_path {
			path.clone()
		} else {
			crate::directories::get_config_file_path()?
		};

		// Ensure the parent directory exists
		if let Some(parent) = config_path.parent() {
			fs::create_dir_all(parent).context(format!(
				"Failed to create config directory: {}",
				parent.display()
			))?;
		}

		// Create clean config for saving (no internal servers)
		let clean_config = self.create_clean_copy_for_saving();
		let config_str =
			toml::to_string(&clean_config).context("Failed to serialize configuration to TOML")?;

		// Write to file
		fs::write(&config_path, config_str).context(format!(
			"Failed to write config to {}",
			config_path.display()
		))?;

		// Update self with the changes (but keep internal servers in memory)
		updater(self);

		Ok(())
	}

	/// REMOVED: create_default_config - use copy_default_config_template instead
	/// This ensures all defaults come from the template file, not code
	///
	/// Update a specific field in the configuration and save to disk
	/// STRICT MODE: Requires existing config file
	pub fn update_specific_field<F>(&mut self, updater: F) -> Result<()>
	where
		F: Fn(&mut Config),
	{
		// Load existing config from disk without initializing internal servers
		let config_path = if let Some(path) = &self.config_path {
			path.clone()
		} else {
			crate::directories::get_config_file_path()?
		};

		let mut disk_config = if config_path.exists() {
			let config_str = fs::read_to_string(&config_path).context(format!(
				"Failed to read config from {}",
				config_path.display()
			))?;
			let mut config: Config =
				toml::from_str(&config_str).context("Failed to parse TOML configuration")?;
			config.config_path = Some(config_path.clone());
			// SIMPLIFIED: Don't initialize internal servers
			config
		} else {
			// STRICT MODE: Fail if no config file exists
			return Err(anyhow::anyhow!(
				"No configuration file found at {}. Run with --init to create a default configuration.",
				config_path.display()
			));
		};

		// Apply the update to the disk config
		updater(&mut disk_config);

		// Validate the updated config
		disk_config.validate()?;

		// Ensure the parent directory exists
		if let Some(parent) = config_path.parent() {
			fs::create_dir_all(parent).context(format!(
				"Failed to create config directory: {}",
				parent.display()
			))?;
		}

		// Create clean config for saving (no internal servers)
		let clean_config = disk_config.create_clean_copy_for_saving();
		let config_str =
			toml::to_string(&clean_config).context("Failed to serialize configuration to TOML")?;

		// Write to file
		fs::write(&config_path, config_str).context(format!(
			"Failed to write config to {}",
			config_path.display()
		))?;

		// Update self with the changes (but keep internal servers in memory)
		updater(self);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Helper function to load and modify the default config template for testing
	fn get_test_config_with_custom_role() -> String {
		// Load the default config template
		let template_content = include_str!("../../config-templates/default.toml");

		// Add a custom "tester" role to the template for testing
		let mut config = template_content.to_string();

		// Add the tester role configuration
		config.push_str(
			r#"

# Test role for unit testing
[[roles]]
name = "tester"
enable_layers = false
temperature = 0.7
layer_refs = []
system = "You are a test assistant."
welcome = "Hello! Test tester role."
mcp = { server_refs = ["octocode", "clt"], allowed_tools = [] }

# Additional test MCP servers for tester role
[[mcp.servers]]
name = "clt"
type = "stdin"
command = "clt"
args = ["mcp"]
timeout_seconds = 30
tools = []
"#,
		);

		config
	}

	#[test]
	fn test_role_parsing() {
		let test_config = get_test_config_with_custom_role();

		// Parse the config
		let mut config: Config = toml::from_str(&test_config).expect("Failed to parse test config");
		config.build_role_map();

		// Verify roles were parsed
		assert_eq!(config.roles.len(), 3);
		assert_eq!(config.role_map.len(), 3);
		assert!(config.role_map.contains_key("tester"));

		let tester_role = config.role_map.get("tester").unwrap();
		assert_eq!(tester_role.mcp.server_refs, vec!["octocode", "clt"]);
		assert!(!tester_role.config.enable_layers);

		// Test get_role_config for custom role
		let (role_config, mcp_config, _, _, _) = config.get_role_config("tester");
		assert!(!role_config.enable_layers);
		assert_eq!(mcp_config.server_refs, vec!["octocode", "clt"]);

		// Test fallback for unknown role
		let (_, mcp_config, _, _, _) = config.get_role_config("unknown");
		assert_eq!(mcp_config.server_refs, Vec::<String>::new()); // Should return empty for unknown roles

		// Test get_merged_config_for_mode for custom role
		let merged_config = config.get_merged_config_for_role("tester");
		// The merged config should only include servers that are referenced by the tester role
		let server_names: Vec<&str> = merged_config.mcp.servers.iter().map(|s| s.name()).collect();
		assert!(server_names.contains(&"octocode"));
		assert!(server_names.contains(&"clt"));
		// Should not contain servers not referenced by the tester role
		assert!(!server_names.contains(&"developer"));
		assert!(!server_names.contains(&"filesystem"));
	}

	#[test]
	fn test_role_merged_config() {
		let test_config = get_test_config_with_custom_role();

		// Parse the config
		let mut config: Config = toml::from_str(&test_config).expect("Failed to parse test config");
		config.build_role_map();

		// Test that the merged config for tester role only includes the specified servers
		let merged_config = config.get_merged_config_for_role("tester");
		// The merged config should only have servers that are in the tester role's server_refs
		let server_names: Vec<&str> = merged_config.mcp.servers.iter().map(|s| s.name()).collect();
		assert!(server_names.contains(&"octocode"));
		assert!(server_names.contains(&"clt"));
		assert!(!server_names.contains(&"developer")); // Should not be included
		assert!(!server_names.contains(&"filesystem")); // Should not be included
	}
}
