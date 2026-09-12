# Paynal API Testing Skill — Installation Guide

## For AI Agent Users

This skill teaches AI agents how to use [Paynal](https://github.com/OscarGPDev/PaynalAPICLI)
to create, execute, and manage API test routines autonomously.

### Option 1: Copy into your project (recommended)

Copy the `.agents/skills/paynal-api-testing/` directory into your own project's
`.agents/skills/` directory:

```bash
cp -r .agents/skills/paynal-api-testing /path/to/your/project/.agents/skills/
```

The agent will automatically discover the skill when working inside your project.

### Option 2: Install globally

Copy to your global Antigravity config:

```bash
cp -r .agents/skills/paynal-api-testing ~/.gemini/config/skills/
```

### Option 3: MCP-only integration

If you only want the MCP tools (no skill instructions), copy just the MCP config:

```bash
# Project-level
cp .agents/skills/paynal-api-testing/mcp_config.json /path/to/your/project/.agents/mcp_config.json

# OR Global
cp .agents/skills/paynal-api-testing/mcp_config.json ~/.gemini/config/mcp_config.json
```

## Prerequisites

- Paynal must be installed and available in your `PATH`:
  ```bash
  # From source
  cargo install --path .

  # Or download a release binary
  # https://github.com/OscarGPDev/PaynalAPICLI/releases
  ```

## What the Agent Gets

With this skill, the agent can:

1. **Create** request and routine YAML files with correct syntax
2. **Execute** API tests via CLI commands or MCP tools
3. **Chain** multi-step workflows with variable captures
4. **Debug** failing assertions with `--dry-run` and `--verbose`
5. **Import** existing collections from Postman, Insomnia, or Bruno
6. **Export** to curl, Postman, or Insomnia formats
7. **Generate** API documentation from collections

## Skill Contents

```
paynal-api-testing/
├── SKILL.md            # Agent instructions (commands, YAML format, tips)
├── mcp_config.json     # MCP server config for direct tool access
├── README.md           # This file
└── examples/
    ├── single-request.yaml   # Single request with captures & assertions
    └── crud-routine.yaml     # Multi-step CRUD routine with chaining
```
