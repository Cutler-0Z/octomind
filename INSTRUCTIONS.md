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
- **Built-in Servers**: developer, filesystem, agent, octocode
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
- Built-in servers: developer, filesystem, agent, octocode

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

## 🎯 WHAT TO LOOK FOR

### When Analyzing the Project
1. **Check memories first** - Look for relevant past work and decisions
2. **Examine config template** - `config-templates/default.toml` shows all available settings
3. **Understand role structure** - How developer/assistant/custom roles are configured
4. **Review MCP servers** - What tools are available and how they're configured
5. **Check layer system** - How query processing and context generation work

### Common Investigation Patterns
- **Configuration issues**: Look in `src/config/` modules
- **Session behavior**: Check `src/session/` directory
- **Tool problems**: Examine `src/mcp/` implementation
- **Provider issues**: Review `src/session/providers/`
- **Layer processing**: Study layered architecture in config and code

### Search Strategies
- Use **multi-term search** for comprehensive results: `['auth', 'login', 'session']`
- Use **single-term search** only for specific identifiers
- Use **GraphRAG** for architectural questions about component relationships

## 🔧 RECENT MAJOR CHANGES

- **Configurable Instructions**: Added `custom_instructions_file_name` setting
- **Strict Config System**: Removed all code defaults, template-based approach
- **Enhanced Tool Routing**: Automatic server type detection
- **Layered Processing**: Query processor → Context generator → Developer
- **Session Optimization**: Enhanced context management and reporting
