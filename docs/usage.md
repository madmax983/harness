# Harness v2 - Usage Guide

## Quick Start

### Basic Usage (No Embeddings)

```bash
# Start with default settings (3 workers, TUI enabled)
cargo run

# Specify number of workers
cargo run -- --workers 5

# Run headless (no TUI)
cargo run -- --no-tui

# Provide initial prompt for Strategoi
cargo run -- --prompt "Build a REST API for user management"
```

### Standalone MCP Daemon (No Orchestrator/TUI)

Use this when you want local Claude/Codex/Gemini instances to share Harness
knowledge without spawning the harness swarm.

```bash
# Start MCP + persistence only (localhost:3000)
cargo run --bin harness-mcpd

# Custom host/port/session file
cargo run --bin harness-mcpd -- \
  --host 127.0.0.1 \
  --port 3000 \
  --session-file .harness-mcp-session
```

Then register the server once in each local agent CLI:

```bash
claude mcp add --transport sse harness http://localhost:3000/sse
codex mcp add harness --url http://localhost:3000/sse
gemini mcp add --scope user -t sse harness http://localhost:3000/sse
```

If a local agent only accepts `command` + `args` MCP config, use a stdio bridge:

```json
{
  "mcpServers": {
    "harness": {
      "command": "cmd",
      "args": ["/c", "npx", "-y", "mcp-remote", "http://localhost:3000/sse"]
    }
  }
}
```

### With Alternative Agent CLIs

Harness supports different LLM CLI tools beyond Claude:

```bash
# Use Gemini CLI instead of Claude
cargo run -- --agent-cli gemini

# Use GPT-4 CLI
cargo run -- --agent-cli gpt-4

# Combine with other options
cargo run -- \
  --agent-cli gemini \
  --workers 3 \
  --prompt "Design microservices architecture"
```

### With Ollama Embeddings (Semantic Search)

First, ensure Ollama is running with your desired model:

```bash
# Pull and run a model (if not already available)
ollama pull nomic-embed-text
ollama run nomic-embed-text
```

Then start Harness with embeddings enabled:

```bash
# Basic embedding support
cargo run -- --embedding-model nomic-embed-text

# Standalone MCP daemon with embeddings
cargo run --bin harness-mcpd -- --embedding-model nomic-embed-text

# With custom Ollama URL
cargo run -- --embedding-model nomic-embed-text --ollama-url http://localhost:11434

# Full example with Gemini and embeddings
cargo run -- \
  --workers 4 \
  --agent-cli gemini \
  --embedding-model nomic-embed-text \
  --prompt "Implement authentication system" \
  --port 3000
```

### Supported Embedding Models

| Model | Dimensions | Use Case |
|-------|------------|----------|
| `nomic-embed-text` | 768 | General-purpose, balanced performance (recommended) |
| `mxbai-embed-large` | 1024 | High accuracy, larger embeddings |
| `all-minilm` | 384 | Fast, smaller embeddings |
| `snowflake-arctic-embed` | 1024 | Code and text understanding |

### CLI Options

```
--workers, -w <NUM>         Number of worker agents (default: 3)
--no-tui                    Disable TUI dashboard
--prompt, -p <TEXT>         Initial prompt/goal for Strategoi
--port <PORT>               MCP server port (default: 3000)
--embedding-model, -e <MODEL>  Ollama model for semantic search
--ollama-url <URL>          Ollama base URL (default: http://localhost:11434)
--agent-cli <CLI>           Agent CLI executable (default: "claude", alternatives: "gemini", "gpt-4")
--agent-runtime <RUNTIME>   Runtime adapter override: claude, codex, gemini, claude_compatible
```

## Architecture

### Without Embeddings
- Knowledge queries (`ask_hive`) return recent entries ordered by time
- Agents share knowledge via `share_knowledge` tool
- Fast, no external dependencies

### With Embeddings
- Knowledge entries are embedded on creation
- Queries use HNSW vector search for semantic similarity
- Returns most relevant knowledge, not just most recent
- Requires Ollama running locally or remotely

## MCP Server

The MCP server endpoint is `http://localhost:3000/sse` (by default) and exposes 14 tools for agent coordination.

Start either runtime:

```bash
# Full swarm runtime (orchestrator + TUI/headless + MCP)
cargo run -- --port 3000

# MCP + persistence only
cargo run --bin harness-mcpd -- --port 3000
```

Example command/args MCP config (Claude Desktop/other stdio-style clients):

```json
{
  "mcpServers": {
    "harness": {
      "command": "cmd",
      "args": ["/c", "npx", "-y", "mcp-remote", "http://localhost:3000/sse"],
      "env": {}
    }
  }
}
```

## Examples

### Example 1: Basic Task Coordination

```bash
# Start with 2 workers
cargo run -- --workers 2 --prompt "Analyze codebase and suggest improvements"
```

The Strategoi will:
1. Register itself and workers
2. Create tasks based on the prompt
3. Assign tasks to workers
4. Workers share discoveries via knowledge
5. Results visible in TUI

### Example 2: BMAD Workflow with Semantic Search

```bash
# Start with full BMAD team + embeddings
cargo run -- \
  --workers 5 \
  --embedding-model nomic-embed-text \
  --prompt "Design and implement user authentication"
```

The team workflow:
1. **Business Analyst** interviews stakeholders, shares requirements
2. **Product Manager** queries relevant requirements via semantic search
3. **Architect** designs system based on discovered knowledge
4. **Developers** implement features
5. **Tester** validates implementation

All knowledge is semantically searchable across the team.

## Troubleshooting

### Ollama Connection Issues

```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# Verify model is available
ollama list | grep nomic-embed-text
```

### MCP Server Not Starting

- Check port is not in use: `netstat -an | grep 3000`
- Check logs for errors: `RUST_LOG=debug cargo run`

### Embedding Dimension Mismatch

If using a custom model not in the supported list, you may see warnings about defaulting to 768 dimensions. The system will work but accuracy may be reduced. The model dimensions are auto-detected for common models.

## Performance Notes

- Embedding generation adds ~50-200ms latency per knowledge entry
- HNSW search is fast (~1-5ms for 1000s of entries)
- Ollama should have 2GB+ RAM for embedding models
- Consider running headless (`--no-tui`) for production

## Next Steps

- See `docs/architecture.md` for system design details
- See `docs/mcp-tools.md` for complete tool reference
- Run integration tests: `cargo test --test integration`
