# OCTOMIND PROJECT GUIDE

## 🎯 CORE PRINCIPLES

### STRICT CONFIG ARCHITECTURE
- **NO FALLBACKS**: Everything defined in config template must be explicitly set
- **NO DEFAULTS IN CODE**: All default values are in `config-templates/default.toml`
- **ENVIRONMENT PRECEDENCE**: Environment variables ALWAYS override config file
- **SECURITY FIRST**: API keys ONLY via environment variables, never in config files

## 🏗️ PROJECT ARCHITECTURE

### SESSION-FIRST APPROACH
- Everything happens in interactive AI sessions
- No separate indexing or search commands
- MCP tools integrated for development operations

### ROLE-BASED CONFIGURATION
- **Developer Role**: Full tools, layered processing, project context
- **Assistant Role**: Chat-only, minimal tools, lightweight
- **Custom Roles**: Inherit from assistant, configurable capabilities

### LAYERED PROCESSING SYSTEM
1. **Query Processor** → Refines user requests
2. **Context Generator** → Collects project information
3. **Developer Layer** → Executes actual work

### MCP TOOL ARCHITECTURE
- **Built-in Servers**: developer, filesystem, web, agent, octocode
- **Server Registry**: Centralized configuration in `[mcp.servers]`
- **Role References**: Roles reference servers via `server_refs`

## 📁 KEY FILES TO EXAMINE

### Configuration System
- `config-templates/default.toml` - Master template with all defaults
- `src/config/mod.rs` - Main config structures and logic
- `src/config/loading.rs` - Config loading and template injection

### Session Management
- `src/session/` - Core session handling
- `src/session/providers/` - AI provider implementations
- `src/session/layers/` - Layered processing system

### MCP Integration
- `src/mcp/` - MCP protocol implementation
- Built-in servers: developer, filesystem, web, agent, octocode

## 🔧 CONFIGURATION APPROACH

### Template-Based Configuration
- All defaults in `config-templates/default.toml`
- No hardcoded defaults in source code
- When no config exists, template is copied automatically
- All settings must be explicitly defined

### Required Configuration Elements
```toml
version = 1  # DO NOT MODIFY
model = "provider:model"  # Single system-wide model
log_level = "none|info|debug"
custom_instructions_file_name = "INSTRUCTIONS.md"
```

### Model Format
Always use `provider:model` format:
- `openrouter:anthropic/claude-sonnet-4`
- `openai:gpt-4o`
- `anthropic:claude-3-5-sonnet`

## 🎯 WHERE TO LOOK FIRST - SPECIFIC GUIDANCE

### 🔍 ALWAYS START HERE
1. **Check memories first** - `remember("your search terms")` for past work and decisions
2. **Examine config template** - `config-templates/default.toml` shows all available settings
3. **Check documentation** - `doc/` directory for comprehensive guides
4. **Use rg for exact lookups** - Never use semantic_search for function names or symbols

### 🛠️ WORKING WITH CONFIGURATION
**FIRST LOOK:**
- `config-templates/default.toml` - Master template with ALL defaults
- `src/config/mod.rs` - Main config structures and validation
- `src/config/loading.rs` - Config loading, merging, and template injection

**SPECIFIC ISSUES:**
- **Role configuration**: `src/config/roles.rs` + template `[[roles]]` sections
- **MCP server setup**: `src/config/mcp.rs` + template `[[mcp.servers]]` sections
- **Tool filtering patterns**: `src/config/mcp.rs` → `expand_patterns_for_server()`
- **Provider setup**: `src/config/providers.rs` + `src/session/providers/`
- **Layer configuration**: `src/session/layers/` + template `[[layers]]` sections

### 🎮 WORKING WITH SESSIONS
**FIRST LOOK:**
- `src/session/mod.rs` - Main session orchestration
- `src/session/chat/` - Core chat session handling
- `src/session/layers/` - Layered processing system

**SPECIFIC ISSUES:**
- **Session commands**: `src/session/chat/session/commands.rs`
- **Context management**: `src/session/chat/context_truncation.rs`
- **Message handling**: `src/session/chat/input.rs` and `src/session/chat/response.rs`
- **Layer processing**: `src/session/layers/generic.rs`
- **Session state**: `src/session/chat/session/mod.rs`

### 🔧 WORKING WITH MCP TOOLS
**FIRST LOOK:**
- `src/mcp/mod.rs` - Main MCP dispatcher and tool routing
- `src/mcp/*/functions.rs` - Tool function definitions for each server

**SPECIFIC SERVERS:**
- **Developer tools**: `src/mcp/dev/` (shell, code analysis)
- **Filesystem tools**: `src/mcp/fs/` (text_editor, list_files)
- **Web tools**: `src/mcp/web/` (web_search, image_search, video_search, news_search, read_html)
- **Agent tools**: `src/mcp/agent/` (task routing to AI layers)

**TOOL ISSUES:**
- **Tool routing**: `src/mcp/mod.rs` → `try_execute_tool_call()`
- **Tool definitions**: `src/mcp/*/functions.rs` files
- **Tool execution**: `src/mcp/*/core.rs` or individual tool files
- **Tool filtering**: `src/mcp/mod.rs` → `filter_tools_by_patterns()` and pattern matching
- **Server health**: `src/mcp/health_monitor.rs`

### 🤖 WORKING WITH AI PROVIDERS
**FIRST LOOK:**
- `src/session/providers/mod.rs` - Provider trait and factory
- `src/session/providers/` - Individual provider implementations

**SPECIFIC PROVIDERS:**
- **OpenRouter**: `src/session/providers/openrouter.rs`
- **OpenAI**: `src/session/providers/openai.rs`
- **Anthropic**: `src/session/providers/anthropic.rs`
- **Google**: `src/session/providers/google.rs`
- **Amazon**: `src/session/providers/amazon.rs`
- **Cloudflare**: `src/session/providers/cloudflare.rs`

### 📊 WORKING WITH LAYERS
**FIRST LOOK:**
- `src/session/layers/generic.rs` - Main layer implementation
- `src/session/layers/mod.rs` - Layer orchestration
- `config-templates/default.toml` - Layer configurations

**LAYER TYPES:**
- **Query Processor**: Refines user input (output_mode: "none")
- **Context Generator**: Gathers project context (output_mode: "replace")
- **Developer**: Main processing layer (output_mode: "append")
- **Reducer**: Context compression (output_mode: "replace")

### 🐛 DEBUGGING SPECIFIC ISSUES

#### Configuration Not Loading
1. `src/config/loading.rs` → `load_config_with_template()`
2. `src/config/mod.rs` → `Config::from_file()`
3. Check environment variable overrides in `src/config/loading.rs`

#### Tool Not Working
1. `src/mcp/mod.rs` → `build_tool_server_map()` (tool routing)
2. `src/mcp/mod.rs` → `try_execute_tool_call()` (execution)
3. `src/mcp/mod.rs` → `filter_tools_by_patterns()` (pattern filtering)
4. Specific tool server in `src/mcp/*/` directories
5. Tool function definition in `src/mcp/*/functions.rs`
6. Check `allowed_tools` patterns in role/layer config

#### Session Behavior Issues
1. `src/session/chat/session/mod.rs` → main session loop
2. `src/session/chat/response.rs` → response generation
3. `src/session/layers/generic.rs` → layer processing
4. `src/session/chat/context_truncation.rs` → context management

#### Provider/Model Issues
1. `src/session/providers/` → specific provider implementation
2. `src/session/providers/mod.rs` → provider factory and trait
3. Check API key environment variables
4. Model format validation in provider code

#### Layer Processing Issues
1. `src/session/layers/generic.rs` → `execute()` method
2. `src/session/layers/mod.rs` → layer orchestration
3. Layer configuration in `config-templates/default.toml`
4. Input/output mode handling in layer execution

## 🚀 COMMON DEVELOPMENT TASKS

### Adding a New MCP Tool
1. **Choose server**: Determine which server (`dev`, `fs`, `web`, `agent`) or create new
2. **Function definition**: Add to `src/mcp/*/functions.rs`
3. **Implementation**: Add to `src/mcp/*/` (core.rs or dedicated file)
4. **Tool routing**: Update `src/mcp/mod.rs` → `try_execute_tool_call()`
5. **Documentation**: Update `doc/06-advanced.md` tool reference

### Adding a New AI Provider
1. **Provider trait**: Implement in `src/session/providers/new_provider.rs`
2. **Factory registration**: Add to `src/session/providers/mod.rs`
3. **Configuration**: Add provider-specific config to template
4. **Documentation**: Update `doc/04-providers.md`

### Adding a New Configuration Option
1. **Template first**: Add to `config-templates/default.toml`
2. **Struct definition**: Add to appropriate `src/config/*.rs` file
3. **Loading logic**: Update `src/config/loading.rs` if needed
4. **Validation**: Add validation in config struct
5. **Documentation**: Update `doc/03-configuration.md`

### Adding a New Layer Type
1. **Configuration**: Add layer config to template `[[layers]]`
2. **Implementation**: Use `src/session/layers/generic.rs` (no new code needed)
3. **Integration**: Configure input_mode, output_mode, and MCP access
4. **Documentation**: Update `doc/07-command-layers.md`

### Debugging Performance Issues
1. **Session timing**: Check `src/session/chat/response.rs` timing code
2. **Tool execution**: Check `src/mcp/mod.rs` execution timing
3. **Provider latency**: Check individual provider implementations
4. **Context size**: Check `src/session/chat/context_truncation.rs`

### Debugging Memory/Token Issues
1. **Context truncation**: `src/session/chat/context_truncation.rs`
2. **Token estimation**: `src/session/mod.rs` → `estimate_tokens()`
3. **Large response handling**: `src/mcp/mod.rs` → `handle_large_response()`
4. **Cache management**: `src/session/cache/` directory

## 📋 QUICK REFERENCE

### Key File Patterns
- **Configuration**: `src/config/*.rs` + `config-templates/default.toml`
- **Session Logic**: `src/session/chat/` + `src/session/layers/`
- **MCP Tools**: `src/mcp/*/functions.rs` + `src/mcp/*/core.rs`
- **AI Providers**: `src/session/providers/*.rs`
- **Documentation**: `doc/*.md` files

### Environment Variables to Check
- **API Keys**: `*_API_KEY` variables for each provider
- **Config Overrides**: `OCTOMIND_*` variables for any config setting
- **Special Keys**: `BRAVE_API_KEY` for web search functionality

### Testing Approach
- **Unit tests**: `cargo test` for individual components
- **Integration**: Test with actual config and sessions
- **Tool testing**: Use `/loglevel debug` in sessions for detailed output
- **Provider testing**: Test with different models and providers
