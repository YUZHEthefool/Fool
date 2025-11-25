# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Fool Shell is a **state-machine driven shell** with **native AI integration**, written in Rust. It parses commands using a deterministic finite automaton (DFA) and provides seamless AI assistance through OpenAI-compatible APIs.

Key features:
- DFA-based command parsing with support for pipes, redirections, and quote handling
- AI integration triggered by `!` prefix (configurable)
- Streaming AI responses with context-aware history
- Syntax highlighting and auto-completion via rustyline
- Built-in commands (cd, pwd, export, alias, etc.)
- Persistent command history with exit codes

## Build & Development Commands

```bash
# Build release version
cargo build --release

# Build debug version (faster compilation, useful during development)
cargo build

# Run in development mode
cargo run

# Run with a single command (non-interactive)
cargo run -- -c "ls -la"

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_simple_command

# Check code (faster than build, catches errors)
cargo check

# Lint with clippy
cargo clippy

# Format code
cargo fmt

# Generate and view documentation
cargo doc --open
```

## Architecture

### Core Data Flow

```
User Input → Parser (DFA) → Executor → Process/Builtin
                ↓
           AIQuery → AiAgent → OpenAI API (streaming)
```

### Module Responsibilities

**[parser.rs](src/parser.rs)** - State machine-based parser
- Implements DFA with states: Idle, CommandStart, Argument, SingleQuote, DoubleQuote, Pipe, RedirectOut/Append/In, AIMode, Escape
- Returns `ParseResult`: Commands (regular shell commands), AIQuery (triggered by `!`), Empty, or Error
- Handles quote escaping, pipe validation, and redirect syntax checking
- Key issue fixed: Now validates that pipes and redirections are complete (not hanging)

**[executor.rs](src/executor.rs)** - Command execution engine
- Executes both built-in commands and external processes
- Manages pipeline creation with proper stdin/stdout chaining
- Handles I/O redirections (`<`, `>`, `>>`)
- Maintains environment variables and aliases
- Built-in commands: cd (with `~` and `-` support), pwd, export, unset, alias, history, clear, help, exit
- Key issue fixed: Exit codes now properly persisted to history

**[ai.rs](src/ai.rs)** - AI integration
- Streams responses from OpenAI-compatible APIs using Server-Sent Events (SSE)
- Shows loading spinner while waiting for response
- Builds context messages from history (configurable via `context_lines`)
- Key issue fixed: Proper SSE buffering to handle chunks split across HTTP boundaries

**[history.rs](src/history.rs)** - Command history management
- Stores commands with timestamps, exit codes, and working directory
- Supports both file-based and memory-only modes
- Memory-only mode used for `-c` flag (non-interactive mode) to avoid write errors on read-only filesystems
- Formats history as AI context (alternating user/assistant messages)
- Implements compaction to prevent unbounded file growth
- Key issue fixed: Compaction now runs periodically (every 100 entries beyond max) to limit file growth

**[repl.rs](src/repl.rs)** - Interactive shell interface
- Uses rustyline for line editing with custom helper (FoolHelper)
- Provides syntax highlighting (commands, flags, variables, strings, operators)
- Implements file path completion and history hints
- Validates unclosed quotes and provides multiline editing
- Displays colorful prompt with username and current directory

**[config.rs](src/config.rs)** - Configuration management
- Loads from `~/.config/fool/config.toml`
- Falls back to defaults if config doesn't exist
- API key priority: config file > FOOL_AI_KEY env > OPENAI_API_KEY env
- Sections: `[ui]`, `[history]`, `[ai]`

**[main.rs](src/main.rs)** - Entry point
- Parses CLI arguments: `-h`, `-v`, `-c <command>`, `--init-config`
- Non-interactive mode (`-c`) uses memory-only history to avoid filesystem issues
- Loads .env file if present
- Creates and runs REPL for interactive mode

## Configuration

Config file location: `~/.config/fool/config.toml`

Generate default config:
```bash
./target/release/fool --init-config
```

Key configuration options:
- `ai.trigger_prefix`: Character(s) to trigger AI mode (default: `"!"`)
- `ai.api_base`: API endpoint URL (supports OpenAI, Azure, local models via Ollama, etc.)
- `ai.api_key`: API key (can also use FOOL_AI_KEY or OPENAI_API_KEY env vars)
- `ai.model`: Model name (default: `"gpt-4o"`)
- `ai.context_lines`: Number of recent commands to include in AI context (default: 10)
- `ai.temperature`: Model temperature (default: 0.7)
- `history.max_entries`: Maximum history size (default: 10000)

## Testing

The codebase has unit tests in each module. Key test areas:
- Parser: Simple commands, pipes, redirects, quotes, AI triggers, malformed input
- Executor: Built-in commands, environment variables, aliases
- History: Add/retrieve entries, max entries enforcement, search functionality
- Config: Default values, TOML parsing

When adding features, ensure proper test coverage, especially for parser state transitions.

## Recent Bug Fixes

Reference: [qa/FIXES.md](qa/FIXES.md)

1. **Exit codes not persisted**: Fixed `update_last_exit_code` to call `compact()` for disk persistence
2. **Non-interactive -c mode failures**: Now uses `History::new_memory_only()` to avoid write errors on read-only locations
3. **Parser accepts malformed input**: Added final state validation for incomplete pipes/redirections
4. **History file unbounded growth**: Implemented periodic compaction (every 100 entries beyond max)
5. **AI streaming parser drops chunks**: Fixed SSE parsing with proper buffering for chunks split across boundaries

## Development Guidelines

### Parser Changes
When modifying the parser state machine:
- Update the `ParserState` enum if adding new states
- Ensure all state transitions are handled in the main `parse_commands` loop
- Add final state validation to catch incomplete syntax
- Write tests for both valid and invalid inputs
- Check quote handling (single, double, escaped) still works

### Executor Changes
When adding built-in commands or modifying execution:
- Add new builtins to `BuiltinCommand` enum and `from_str` match
- Implement the builtin in `execute_builtin`
- Ensure proper exit code handling
- Test with pipelines and redirections to ensure they still work

### AI Integration Changes
- The AI agent expects OpenAI v1 API format (chat completions endpoint)
- Context is built from history entries formatted as user/assistant pairs
- Streaming uses SSE (Server-Sent Events) with "data: " prefix
- Handle `[DONE]` message to terminate stream
- Buffer incomplete JSON chunks across HTTP boundaries

### History Changes
- History entries are stored as newline-delimited JSON
- Always update both in-memory and on-disk state
- Memory-only mode should skip all file I/O operations
- Consider performance: compaction is expensive, run sparingly

## Binary Location

After building:
- Debug: `target/debug/fool`
- Release: `target/release/fool`

Install to system (optional):
```bash
cp target/release/fool ~/.local/bin/
```
