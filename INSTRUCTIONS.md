# OCTOMIND DEVELOPMENT GUIDE

## 🎯 CORE ARCHITECTURE

**Session-First Design**: Everything happens in interactive AI sessions with MCP tools
**Template-Based Config**: All defaults in `config-templates/default.toml`, NO hardcoded values
**Role-Based Access**: Developer (full tools), Assistant (chat only), Custom roles
**Layered Processing**: query_processor → context_generator → developer layers
**Custom Commands**: `/run <command>` executes configured layer-based commands
**Agent System**: `agent_<name>(task="...")` MCP tools route tasks to specialized AI layers
**Cache & Cost**: 2-marker cache system + automatic cost tracking across sessions/layers/tools

## 📍 WHERE TO LOOK BY TASK

### 🔧 CONFIGURATION ISSUES
**Template & Loading:**
- `config-templates/default.toml` - ALL defaults and structure
- `src/config/loading.rs` - Config loading, template injection, env overrides
- `src/config/mod.rs` - Main Config struct and validation

**Specific Config Types:**
- **Roles**: `src/config/roles.rs` + template `[[roles]]` sections
- **MCP Servers**: `src/config/mcp.rs` + template `[[mcp.servers]]` sections
- **Layers**: `src/session/layers/layer_trait.rs` + template `[[layers]]` sections
- **Commands**: Template `[[commands]]` sections (use same layer system)
- **Agents**: `src/config/mod.rs` AgentConfig + template (route to layers via MCP)
- **Providers**: `src/providers/` directory

### 🎮 SESSION BEHAVIOR
**Core Session Logic:**
- `src/session/chat/session/runner.rs` - Main interactive session loop
- `src/session/chat/session/commands/` - All `/command` implementations
- `src/session/chat/response.rs` - Response processing orchestrator

**Context & Memory:**
- `src/session/chat/context_truncation.rs` - Smart context management
- `src/session/cache.rs` - Caching system (2-marker approach)
- `src/session/chat/input.rs` - User input handling with history

**Cost & Performance:**
- `src/session/chat/cost_tracker.rs` - Cost accumulation across sessions/layers/tools
- `src/session/mod.rs` SessionInfo - Token/cost tracking per session
- Auto cache markers: system messages, tools (supports caching models only)
- 2-marker system: content cache management for cost optimization

### 🔧 MCP TOOLS
**Tool System Core:**
- `src/mcp/mod.rs` - Tool routing, execution, `try_execute_tool_call()`
- `src/mcp/tool_map.rs` - Static tool-to-server mapping for performance

**Built-in Tool Servers:**
- **Developer**: `src/mcp/dev/` (shell execution)
- **Filesystem**: `src/mcp/fs/` (text_editor, list_files)
- **Web**: `src/mcp/web/` (web_search, read_html, image_search)
- **Agent**: `src/mcp/agent/` (layer routing for AI tasks via `agent_<name>` tools)

**External Server Management:**
- `src/mcp/server.rs` - HTTP server communication
- `src/mcp/process.rs` - Stdin server process management
- `src/mcp/health_monitor.rs` - Server health monitoring

### 🤖 AI PROVIDERS
**Provider System:**
- `src/providers/mod.rs` - Provider trait and factory
- `src/providers/*/` - Individual provider implementations (OpenRouter, OpenAI, Anthropic, etc.)

### 📊 LAYERED PROCESSING & COMMANDS
**Layer Architecture:**
- `src/session/layers/types/generic.rs` - Main layer implementation
- `src/session/layers/orchestrator.rs` - Layer execution orchestration
- `src/session/layers/layer_trait.rs` - Layer configuration and traits

**Custom Commands:**
- `src/session/chat/command_executor.rs` - `/run <command>` execution
- Template `[[commands]]` sections - Command definitions (use layer system)
- Commands execute layers without storing in session history

**Agent System:**
- `src/mcp/agent/functions.rs` - Dynamic `agent_<name>` tool generation
- `src/config/mod.rs` AgentConfig - Agent-to-layer routing configuration
- Template agents section - Agent definitions that route to layers via MCP

## 🐛 DEBUGGING BY SYMPTOM

### Tool Not Working
1. **Check tool routing**: `src/mcp/mod.rs` → `build_tool_server_map()`
2. **Check tool execution**: `src/mcp/mod.rs` → `try_execute_tool_call()`
3. **Check tool definitions**: `src/mcp/*/functions.rs` files
4. **Check allowed_tools patterns**: Role/layer config in template

### Configuration Not Loading
1. **Template injection**: `src/config/loading.rs` → `load()`
2. **Environment overrides**: Check `OCTOMIND_*` variables
3. **Validation errors**: `src/config/validation.rs` → `validate()`

### Session Commands Failing
1. **Command routing**: `src/session/chat/session/commands/mod.rs`
2. **Individual commands**: `src/session/chat/session/commands/*.rs`
3. **Command permissions**: Check role configuration

### Provider/Model Issues
1. **Provider implementation**: `src/providers/*/` specific provider
2. **API keys**: Check `*_API_KEY` environment variables
3. **Model format**: Must be `provider:model` format

### Layer Processing Issues
1. **Layer execution**: `src/session/layers/types/generic.rs` → `process()`
2. **Layer orchestration**: `src/session/layers/orchestrator.rs`
3. **Input/output modes**: Check layer config in template

### Cache/Cost Issues
1. **Cache markers**: `src/session/cache.rs` → `manage_content_cache_markers()`
2. **Cost tracking**: `src/session/chat/cost_tracker.rs` → `track_exchange_cost()`
3. **Token counting**: `src/session/mod.rs` SessionInfo struct
4. **Cache support**: Check if model supports caching (Anthropic Claude, etc.)

## 🚀 COMMON MODIFICATIONS

### Add New MCP Tool
1. **Function definition**: Add to `src/mcp/*/functions.rs`
2. **Implementation**: Add to `src/mcp/*/` (core.rs or new file)
3. **Routing**: Update `src/mcp/mod.rs` → `try_execute_tool_call()`

### Add New Provider
1. **Implementation**: Create `src/providers/new_provider.rs`
2. **Registration**: Add to `src/providers/mod.rs` factory
3. **Config**: Add provider section to template

### Add New Configuration
1. **Template first**: Add to `config-templates/default.toml`
2. **Struct**: Add to appropriate `src/config/*.rs`
3. **Validation**: Add validation rules

### Add New Layer
1. **Config**: Add to template `[[layers]]` section
2. **No code needed**: Uses generic layer implementation
3. **Configure**: Set input_mode, output_mode, MCP access

### Add New Custom Command
1. **Config**: Add to template `[[commands]]` section (same as layer config)
2. **Usage**: `/run <command_name>` executes the layer without storing in history
3. **No code needed**: Uses existing command executor + layer system

### Add New Agent
1. **Layer**: First create the layer in `[[layers]]` section
2. **Agent config**: Add to template agents section with layer name
3. **Usage**: `agent_<name>(task="...")` MCP tool routes to the layer

## 📋 CRITICAL PATTERNS

### File Patterns
- **Config**: `src/config/*.rs` + `config-templates/default.toml`
- **Tools**: `src/mcp/*/functions.rs` + `src/mcp/*/core.rs`
- **Sessions**: `src/session/chat/` + `src/session/layers/`
- **Providers**: `src/providers/*/`

### Environment Variables
- **API Keys**: `OPENROUTER_API_KEY`, `OPENAI_API_KEY`, `BRAVE_API_KEY`, etc.
- **Config Overrides**: `OCTOMIND_*` for any config setting
- **Debug**: Use `/loglevel debug` in sessions

### Key Commands
- **Config**: `octomind config --show` to see current config
- **Sessions**: `octomind session` for interactive mode
- **Debug**: `/mcp info` to check tool status in sessions
- **Custom Commands**: `/run <command_name>` to execute configured layers
- **Agents**: Use `agent_<name>(task="description")` MCP tools for specialized AI tasks
- **Cache**: `/cache` to manually mark cache points, `/info` to see costs/tokens
