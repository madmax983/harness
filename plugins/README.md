# Harness Plugins for Claude Code

This directory contains Claude Code plugins that enhance the harness development experience.

## Harness Plugin

**Location:** `plugins/harness/`

Seamless integration with harness hive mind for multi-agent coordination, task tracking, and knowledge sharing.

### Installation

Copy the plugin to your desired location:

**User-level (recommended - available in all projects):**
```bash
cp -r plugins/harness ~/.claude/plugins/harness
```

**Project-level (only for this project):**
```bash
cp -r plugins/harness .claude/plugins/harness
```

### Features

- 🤖 Auto-registers as strategoi agent on session start
- 📋 Smart task detection from user requests
- 💡 Knowledge sharing prompts after completions
- ⚡ Quick commands: `/hive`, `/task`, `/agents`
- 🎯 Multi-agent workflow skill

### Quick Start

1. Install the plugin (see above)
2. Restart Claude Code
3. You should see: "Registering as strategoi agent in harness hive..."
4. Try `/hive` to see current status
5. Use `/task create "Your task"` to track work

### Commands

- `/hive [--verbose]` - Show hive status (agents, tasks, knowledge)
- `/task create/claim/complete/list` - Quick task management
- `/agents list/spawn/message` - Multi-agent coordination

### Configuration

Create `~/.claude/plugins/harness.local.md` (user-level) or `.claude/plugins/harness.local.md` (project-level):

```yaml
---
auto_register: true
auto_parse_tasks: true
auto_share_knowledge: true
show_hive_in_status: false
---
```

### Documentation

See `plugins/harness/README.md` for full documentation.

## Contributing

To add new plugins or improve existing ones, submit a PR with:
- Plugin in `plugins/<name>/`
- Updated README
- Documentation in the plugin's README.md
