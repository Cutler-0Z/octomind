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

#[derive(Args, Debug)]
pub struct RunArgs {
	/// Input to process with AI
	#[arg(value_name = "INPUT")]
	pub input: String,

	/// Name of the session to start or resume
	#[arg(long, short)]
	pub name: Option<String>,

	/// Resume an existing session
	#[arg(long, short)]
	pub resume: Option<String>,

	/// Use a specific model instead of the one configured in config (runtime only, not saved)
	#[arg(long)]
	pub model: Option<String>,

	/// Temperature for the AI response (0.0 to 1.0, runtime only, not saved)
	#[arg(long, default_value = "0.7")]
	pub temperature: f32,

	/// Session role: developer (default with layers and tools) or assistant (simple chat without tools)
	#[arg(long, default_value = "developer")]
	pub role: String,
}

impl RunArgs {
	/// Convert RunArgs to SessionArgs for reusing session infrastructure
	pub fn to_session_args(&self) -> super::SessionArgs {
		super::SessionArgs {
			name: self.name.clone(),
			resume: self.resume.clone(),
			model: self.model.clone(),
			temperature: self.temperature,
			role: self.role.clone(),
		}
	}
}
