---
name: paynal-api-testing
description: >-
  Use this skill when the user asks to create, execute, debug, or manage API
  requests and test routines using Paynal (paynalapicli). Activate when you need
  to: run ad-hoc or YAML-based API tests, create request/routine YAML files,
  chain multi-step API workflows with variable captures, discover or run routes
  across global workspaces, import/export collections, or interact with the
  Paynal MCP server.
---

# Paynal API Testing Skill

Paynal (`paynal`) is a fast, CLI-first API client, multi-step routine runner, and AI agent testing engine written in Rust. Requests and routines are stored as Git-friendly YAML files inside `collections/`.

> [!CAUTION]
> **Never execute `paynal ui` or `paynal tui` as an AI agent.** It launches an interactive full-screen terminal UI (Ratatui) that waits indefinitely for human keyboard input and will hang your command session. Only mention `paynal ui` if the user asks how to open the visual dashboard themselves.

---

## 1. Fast Ad-Hoc Requests (`paynal req`)

When you need to test an endpoint quickly without creating a YAML file first, use `paynal req` (supports `${VAR}` interpolation from `paynal.env`/`.env`, assertions, and `--save`):

```bash
# Simple GET request
paynal req https://api.example.com/health

# POST with JSON body, headers, bearer token, and inline assertions
paynal req POST "${BASE_URL}/v1/auth/login" \
  -H "Content-Type: application/json" \
  -b '{"email":"${USER_EMAIL}","password":"${USER_PASSWORD}"}' \
  --status 200 \
  --assert-json "$.success=true"

# Preview interpolated request without sending network traffic
paynal req GET "${BASE_URL}/v1/items" -p "limit=10" --dry-run

# Execute and save directly as collections/users/create.yaml
paynal req POST "${BASE_URL}/v1/users" -b '{"name":"Alice"}' --save users/create
```

---

## 2. Essential CLI Commands

| Goal | Command |
| --- | --- |
| Initialize workspace (`paynal.json`, `collections/`) | `paynal init` |
| Generate single request YAML template | `paynal add <folder/name> -X POST -b json` |
| Generate multi-step routine YAML template | `paynal add <folder/name> --routine` |
| Validate & preview YAML without network calls | `paynal exec <path> --dry-run` |
| Execute request or routine (with full details) | `paynal exec <path> --verbose` |
| Execute with machine-parseable JSON output | `paynal exec <path> --reporter json` |
| Override variables or environment profile | `paynal exec <path> -E staging --var KEY=VAL` |
| Enforce strict CI checks | `paynal exec <path> --fail-fast --strict-vars` |
| Discover routes across all registered workspaces | `paynal global discover [query] --json` |
| Execute a route from another registered workspace | `paynal exec @<workspace>/<path>` |
| Refresh embedded AI skill across workspaces | `paynal update --sync-skill` |

### Exit Codes
- `0`: All requests and assertions passed.
- `1`: One or more assertions failed.
- `2`: Runtime/configuration error (missing file, invalid YAML schema, unresolved `--strict-vars`, or network error).

---

## 3. Minimal Valid YAML Templates

> [!IMPORTANT]
> Paynal uses `#[serde(deny_unknown_fields)]`. Always include `version: "1.0"` and `name:` at root, place `auth`/`params` inside `request:`, and keep `body:` as a string.

**Single Request (`collections/health.yaml`):**
```yaml
version: "1.0"
name: "Health Check"
request:
  method: "GET"
  url: "${BASE_URL}/health"
assert:
  status: 200
  json:
    "$.status": "ok"
```

**Multi-Step Routine (`collections/auth/flow.yaml`):**
```yaml
version: "1.0"
name: "Login and Fetch Profile"
steps:
  - id: "login"
    request:
      method: "POST"
      url: "${BASE_URL}/auth/login"
      headers:
        Content-Type: "application/json"
      body: '{"email": "${EMAIL}", "password": "${PASSWORD}"}'
    capture:
      token: "$.data.token"
    assert:
      status: 200
  - id: "profile"
    request:
      method: "GET"
      url: "${BASE_URL}/me"
      auth:
        type: bearer
        token: "${token}"
    assert:
      status: 200
```

---

## 4. Detailed Reference Guides (Read On-Demand)

To keep context light, consult these reference files only when your task requires them:

- **[YAML Schema, Chaining & Assertions](./references/yaml-schema-and-assertions.md)** — Read when writing or debugging complex `.yaml` files, authentication (`bearer`, `basic`, `apikey`), retries, regex/header captures, dynamic variables (`${$uuid}`, `${$timestamp}`), `output_sections`, or advanced assertions (`status_range`, `json_gt`, `schema`, etc.).
- **[Multi-Workspace (`paynal global`) & MCP Server](./references/multi-workspace-and-mcp.md)** — Read when working across multiple microservice repositories (`@workspace/path`, `-C <workspace>`, `paynal global`) or configuring/calling the `paynal mcp` server (6 tools).
- **[Import, Export, Docs & CI/CD Reporters](./references/import-export-and-ci.md)** — Read when importing from Postman/Insomnia/Bruno, exporting to `curl`/Postman/Insomnia, generating Markdown docs (`paynal doc`), or setting up parallel CI/JUnit test runs.
