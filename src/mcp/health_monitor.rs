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

// Background health monitoring for MCP servers

use super::process::{self, is_server_running, ServerHealth};
use crate::config::{Config, McpConnectionType, McpServerConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

// Global flag to control the health monitor
static HEALTH_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

// Health monitoring configuration
const HEALTH_CHECK_INTERVAL_SECONDS: u64 = 30; // Check every 30 seconds

/// Start the background health monitoring task
pub async fn start_health_monitor(config: Arc<Config>) -> Result<(), anyhow::Error> {
	// Prevent multiple health monitors from running
	if HEALTH_MONITOR_RUNNING
		.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
		.is_err()
	{
		crate::log_debug!("Health monitor is already running");
		return Ok(());
	}

	crate::log_debug!(
		"Starting MCP server health monitor (checking every {}s)",
		HEALTH_CHECK_INTERVAL_SECONDS
	);

	// Get external servers that need monitoring (all external servers, but only restart local ones)
	let external_servers: Vec<McpServerConfig> = config
		.mcp
		.servers
		.iter()
		.filter(|server| {
			matches!(
				server.connection_type(),
				McpConnectionType::Http | McpConnectionType::Stdin
			)
		})
		.cloned()
		.collect();

	if external_servers.is_empty() {
		crate::log_debug!("No external servers to monitor, health monitor stopping");
		HEALTH_MONITOR_RUNNING.store(false, Ordering::SeqCst);
		return Ok(());
	}

	crate::log_debug!(
		"Health monitor will track {} external servers: {}",
		external_servers.len(),
		external_servers
			.iter()
			.map(|s| {
				let server_type = match s.connection_type() {
					McpConnectionType::Stdin => "stdin",
					McpConnectionType::Http => {
						if s.command().is_some() {
							"http-local"
						} else {
							"http-remote"
						}
					}
					McpConnectionType::Builtin => "builtin",
				};
				format!("{}({})", s.name(), server_type)
			})
			.collect::<Vec<_>>()
			.join(", ")
	);

	// Spawn the monitoring task
	tokio::spawn(async move {
		let mut check_interval = interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECONDS));

		loop {
			// Wait for the next check interval
			check_interval.tick().await;

			// Check if we should stop monitoring
			if !HEALTH_MONITOR_RUNNING.load(Ordering::SeqCst) {
				crate::log_debug!("Health monitor stopping");
				break;
			}

			// Perform health check on all external servers and restart if process is dead
			for server in &external_servers {
				if let Err(e) = check_server_health_and_restart_if_dead(server).await {
					crate::log_debug!("Health monitor error for server '{}': {}", server.name(), e);
				}
			}
		}

		crate::log_debug!("Health monitor task completed");
	});

	Ok(())
}

/// Stop the background health monitoring task
pub fn stop_health_monitor() {
	if HEALTH_MONITOR_RUNNING
		.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
		.is_ok()
	{
		crate::log_debug!("Stopping health monitor");
	}
}

/// Check a single server's health and restart ONLY if process is dead
async fn check_server_health_and_restart_if_dead(
	server: &McpServerConfig,
) -> Result<(), anyhow::Error> {
	// Perform different health checks based on server type
	let health_status = match server.connection_type() {
		McpConnectionType::Stdin => {
			// For stdin servers, check if the process is running
			if is_server_running(server.name()) {
				ServerHealth::Running
			} else {
				ServerHealth::Dead
			}
		}
		McpConnectionType::Http => {
			if server.command().is_some() {
				// Local HTTP server - check if the process is running
				if is_server_running(server.name()) {
					ServerHealth::Running
				} else {
					ServerHealth::Dead
				}
			} else {
				// Remote HTTP server - perform HTTP health check
				match perform_http_health_check(server).await {
					Ok(true) => ServerHealth::Running,
					Ok(false) => ServerHealth::Dead,
					Err(_) => ServerHealth::Dead,
				}
			}
		}
		McpConnectionType::Builtin => {
			// Builtin servers are always running
			ServerHealth::Running
		}
	};

	let restart_info = process::get_server_restart_info(server.name());

	crate::log_debug!(
		"Health check: server '{}' status = {:?}, restart_count = {}",
		server.name(),
		health_status,
		restart_info.restart_count
	);

	// Update health status and last health check time
	{
		let mut restart_info_guard = process::SERVER_RESTART_INFO.write().unwrap();
		let info = restart_info_guard
			.entry(server.name().to_string())
			.or_default();
		info.health_status = health_status;
		info.last_health_check = Some(std::time::SystemTime::now());
	}

	match health_status {
		ServerHealth::Dead => {
			// Server process is actually dead - this is when we should restart
			crate::log_debug!(
				"Health monitor detected dead server '{}' - attempting restart",
				server.name()
			);

			// Check if we should attempt restart (respect max attempts)
			if restart_info.restart_count >= 3 {
				crate::log_debug!(
					"Server '{}' has exceeded max restart attempts ({}), marking as failed",
					server.name(),
					restart_info.restart_count
				);

				// Mark as failed to prevent further restart attempts
				let mut restart_info_guard = process::SERVER_RESTART_INFO.write().unwrap();
				if let Some(info) = restart_info_guard.get_mut(server.name()) {
					info.health_status = ServerHealth::Failed;
				}
				return Ok(());
			}

			// Check cooldown period to avoid rapid restart attempts
			if let Some(last_restart) = restart_info.last_restart_time {
				let time_since_restart = std::time::SystemTime::now()
					.duration_since(last_restart)
					.unwrap_or(std::time::Duration::from_secs(0));

				if time_since_restart < Duration::from_secs(30) {
					crate::log_debug!(
						"Server '{}' is in cooldown period, skipping restart attempt",
						server.name()
					);
					return Ok(());
				}
			}

			// Attempt to restart the dead server
			match restart_dead_server(server).await {
				Ok(()) => {
					crate::log_info!(
						"Health monitor successfully restarted dead server '{}'",
						server.name()
					);
				}
				Err(e) => {
					crate::log_debug!(
						"Health monitor failed to restart dead server '{}': {}",
						server.name(),
						e
					);
				}
			}
		}
		ServerHealth::Failed => {
			// Server has failed - check if enough time has passed to reset failure state
			if let Some(last_restart) = restart_info.last_restart_time {
				let time_since_last_restart = std::time::SystemTime::now()
					.duration_since(last_restart)
					.unwrap_or(std::time::Duration::from_secs(0));

				// Reset failure state after 5 minutes
				if time_since_last_restart > Duration::from_secs(300) {
					crate::log_debug!(
						"Resetting failed state for server '{}' after cooldown period",
						server.name()
					);
					if let Err(e) = process::reset_server_failure_state(server.name()) {
						crate::log_debug!(
							"Failed to reset failure state for server '{}': {}",
							server.name(),
							e
						);
					}
				}
			}
		}
		ServerHealth::Running => {
			// Server is running - verify responsiveness but don't restart on failed responses
			// Failed responses are normal due to misled requests
			if !verify_server_responsiveness(server).await {
				crate::log_debug!(
					"Health monitor: server '{}' process is running but not responsive (this is normal for failed requests)",
					server.name()
				);
				// Don't mark as dead - failed responses are normal
				// Only mark as dead if the actual process is not running
			}
		}
		ServerHealth::Restarting => {
			// Server is currently restarting, just monitor
			crate::log_debug!(
				"Health monitor: server '{}' is currently restarting",
				server.name()
			);
		}
	}

	Ok(())
}

/// Attempt to restart a dead server (only for servers that can be restarted)
async fn restart_dead_server(server: &McpServerConfig) -> Result<(), anyhow::Error> {
	// Check if this server can actually be restarted
	let can_restart = match server.connection_type() {
		McpConnectionType::Stdin => true, // Stdin servers can always be restarted
		McpConnectionType::Http => server.command().is_some(), // Only local HTTP servers can be restarted
		McpConnectionType::Builtin => false, // Builtin servers don't need restart
	};

	if !can_restart {
		crate::log_debug!(
			"Server '{}' is a remote server and cannot be restarted by health monitor",
			server.name()
		);
		return Ok(()); // Not an error - just can't restart remote servers
	}

	crate::log_debug!(
		"Health monitor attempting to restart dead server '{}'",
		server.name()
	);

	match process::ensure_server_running(server).await {
		Ok(_) => {
			crate::log_info!(
				"Health monitor successfully restarted dead server '{}'",
				server.name()
			);
			Ok(())
		}
		Err(e) => {
			crate::log_debug!(
				"Health monitor failed to restart dead server '{}': {}",
				server.name(),
				e
			);
			Err(e)
		}
	}
}

/// Verify that a server is actually responsive (basic health check)
async fn verify_server_responsiveness(server: &McpServerConfig) -> bool {
	// For stdin servers, we can try a simple ping-like operation
	// For HTTP servers, we could do a simple HTTP request
	// BUT: Failed responses are normal due to misled requests
	// We should only check if the PROCESS is alive, not if it responds correctly

	match server.connection_type() {
		McpConnectionType::Stdin => {
			// For stdin servers, just check if the process is alive
			// Don't try to communicate - that might fail due to misled requests
			process::is_server_running(server.name())
		}
		McpConnectionType::Http => {
			// For HTTP servers, just check if the process is running
			// Don't make HTTP requests - failed responses are normal
			process::is_server_running(server.name())
		}
		McpConnectionType::Builtin => {
			// Built-in servers are always "running"
			true
		}
	}
}

/// Get health monitor status
pub fn is_health_monitor_running() -> bool {
	HEALTH_MONITOR_RUNNING.load(Ordering::SeqCst)
}

/// Force a health check on all servers (for manual triggering)
pub async fn force_health_check(config: &Config) -> Result<(), anyhow::Error> {
	crate::log_debug!("Forcing health check on all external servers");

	let external_servers: Vec<McpServerConfig> = config
		.mcp
		.servers
		.iter()
		.filter(|server| {
			matches!(
				server.connection_type(),
				McpConnectionType::Http | McpConnectionType::Stdin
			)
		})
		.cloned()
		.collect();

	for server in &external_servers {
		if let Err(e) = check_server_health_and_restart_if_dead(server).await {
			crate::log_debug!(
				"Force health check error for server '{}': {}",
				server.name(),
				e
			);
		}
	}

	Ok(())
}

/// Perform HTTP health check for remote servers
async fn perform_http_health_check(server: &McpServerConfig) -> Result<bool, anyhow::Error> {
	if let Some(url) = server.url() {
		let client = reqwest::Client::builder()
			.timeout(std::time::Duration::from_secs(5)) // 5 second timeout for health checks
			.build()?;

		// Try to make a JSON-RPC tools/list request to check if server is responding
		let health_url = url.trim_end_matches("/");

		// Use the same header setup as the main server implementation
		let mut headers = reqwest::header::HeaderMap::new();
		headers.insert(
			reqwest::header::CONTENT_TYPE,
			reqwest::header::HeaderValue::from_static("application/json"),
		);

		if let Some(token) = server.auth_token() {
			headers.insert(
				reqwest::header::AUTHORIZATION,
				reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token))?,
			);
		}

		// Use tools/list for health check (same as main functionality)
		let jsonrpc_request = crate::mcp::server::create_tools_list_request();

		match client
			.post(health_url)
			.headers(headers)
			.json(&jsonrpc_request)
			.send()
			.await
		{
			Ok(response) => {
				let is_healthy =
					response.status().is_success() || response.status().is_client_error();
				// Both 2xx and 4xx are considered "server responding" - 5xx or connection errors are not
				crate::log_debug!(
					"HTTP health check for '{}': {} (status: {})",
					server.name(),
					if is_healthy {
						"✅ Healthy"
					} else {
						"❌ Unhealthy"
					},
					response.status()
				);
				Ok(is_healthy)
			}
			Err(e) => {
				crate::log_debug!(
					"HTTP health check for '{}': ❌ Failed - {}",
					server.name(),
					e
				);
				Ok(false)
			}
		}
	} else {
		Err(anyhow::anyhow!("No URL configured for HTTP server"))
	}
}
