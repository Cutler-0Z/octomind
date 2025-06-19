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

// Web search functionality using Brave Search API - Modular exports

// Re-export all search functions from their respective modules
pub use super::image_search::{execute_image_search, get_image_search_function};
pub use super::news_search::{execute_news_search, get_news_search_function};
pub use super::video_search::{execute_video_search, get_video_search_function};
pub use super::web_search::{execute_web_search, get_web_search_function};
