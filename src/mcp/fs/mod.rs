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

// File System MCP provider - modular structure
// Handles file operations

pub mod core;
pub mod directory;
pub mod file_ops;
pub mod functions;
pub mod text_editing;

// Re-export main functionality
pub use core::{execute_list_files, execute_text_editor};
pub use functions::get_all_functions;
