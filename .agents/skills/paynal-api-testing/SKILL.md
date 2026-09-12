---
name: paynal-api-testing
description: >-
  Use this skill when the user asks to create, execute, debug, or manage API
  requests and test routines using Paynal (paynalapicli). Activate when you need
  to: run API tests, create request/routine YAML files, chain multi-step API
  workflows with variable captures, generate API documentation, import/export
  collections (Postman, Insomnia, Bruno, curl), or interact with the Paynal MCP
  server for autonomous API testing.
---

# Paynal API Testing Skill

Paynal (`paynal`) is a high-performance, CLI-first API client, routine runner,
and AI agent testing engine written in Rust. It stores requests and multi-step
routines as human-readable YAML files under `collections/`.

## Quick Reference — CLI Commands

| Command | Description |
| --- | --- |
| `paynal init` | Initialize a new workspace (creates `paynal.json`, `.gitignore`) |
| `paynal add <path>` | Generate a request/routine YAML template |
| `paynal exec <path>` | Execute a request or routine |
| `paynal exec <folder>` | Execute all requests in a folder (parallel) |
| `paynal exec <path> --dry-run` | Preview without sending network requests |
| `paynal exec <path> --verbose` | Show full request/response with masked secrets |
| `paynal exec <path> --reporter json` | Output results as JSON (CI-friendly) |
| `paynal exec <path> --reporter junit --out results.xml` | JUnit XML output |
| `paynal exec <path> --fail-fast` | Abort on first assertion failure |
| `paynal exec <path> --strict-vars` | Error on unresolved `${VAR}` placeholders |
| `paynal exec <path> --var KEY=VAL` | Override/inject variables from CLI |
| `paynal exec <path> --timeout 5000` | Override timeout (ms) |
| `paynal exec <path> -k` | Skip TLS certificate validation |
| `paynal doc <path>` | Generate Markdown API documentation |
| `paynal export <path> --format curl` | Export as curl commands |
| `paynal export <path> --format postman` | Export as Postman collection |
| `paynal import <file>` | Import from Postman, Insomnia, or Bruno |
| `paynal remove <path>` | Delete a request/routine file |
| `paynal clean project` | Remove temp/backup files from workspace |
| `paynal mcp` | Start MCP server (stdio JSON-RPC 2.0) |
| `paynal ui` | Launch interactive TUI dashboard |

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | All requests executed, all assertions passed |
| `1` | Execution completed but one or more assertions failed |
| `2` | Runtime error (file not found, network error, parse error) |

---

## YAML Request Format

A single API request file (`collections/<name>.yaml`):

```yaml
name: "Login"
request:
  method: "POST"
  url: "${baseUrl}/api/auth/login"
  headers:
    Content-Type: "application/json"
  body:
    email: "${EMAIL}"
    password: "${PASSWORD}"
  timeout_ms: 10000

# Optional: Authentication block (auto-generates headers)
auth:
  type: Bearer
  token: "${ACCESS_TOKEN}"
# OR
auth:
  type: Basic
  username: "${USER}"
  password: "${PASS}"
# OR
auth:
  type: ApiKey
  key: "X-API-Key"
  value: "${API_KEY}"
  in: header  # or "query"

# Optional: Structured query parameters (auto URL-encoded)
params:
  page: "1"
  search: "${query}"

# Capture values from responses for chaining
capture:
  token: "$.data.accessToken"         # JSONPath
  sessionId: "header.X-Session-Id"    # Response header
  requestStatus: "status"             # HTTP status code
  elapsed: "duration"                 # Request latency in ms
  rawBody: "body"                     # Full response body
  userId: "regex:\"userId\":\"(\\d+)\""  # Regex capture group

# Assertions
assert:
  status: 200
  # OR ranges / lists:
  status_range: [200, 299]
  status_in: [200, 201, 204]
  json:
    "$.success": true
    "$.data.role": "admin"
  json_length:
    "$.data.items": 10
  json_type:
    "$.data.id": "number"
    "$.data.name": "string"
  json_gt:
    "$.data.count": 0
  json_gte:
    "$.data.count": 1
  json_lt:
    "$.data.errorCount": 5
  header_contains:
    Content-Type: "application/json"
  header_regex:
    X-Request-Id: "^[a-f0-9-]{36}$"
  schema:
    type: object
    required: ["success", "data"]
    properties:
      success: { type: boolean }
      data: { type: object }

# Local variables
vars:
  baseUrl: "https://api.example.com"
```

---

## YAML Routine Format (Multi-Step Chaining)

A routine chains multiple requests with variable propagation:

```yaml
name: "Auth Flow"
type: routine
vars:
  baseUrl: "https://api.example.com"

steps:
  - id: "login"
    vars:
      customVar: "step-local-value"
    request:
      method: POST
      url: "${baseUrl}/auth/login"
      headers:
        Content-Type: "application/json"
      body:
        email: "${EMAIL}"
        password: "${PASSWORD}"
    capture:
      token: "$.data.token"
      userId: "$.data.userId"
    assert:
      status: 200

  - id: "get-profile"
    request:
      method: GET
      url: "${baseUrl}/users/${userId}"
      headers:
        Authorization: "Bearer ${token}"
    assert:
      status: 200
      json:
        "$.data.email": "${EMAIL}"

  - id: "update-profile"
    request:
      method: PUT
      url: "${baseUrl}/users/${userId}"
      headers:
        Authorization: "Bearer ${token}"
        Content-Type: "application/json"
      body:
        displayName: "Updated Name"
    assert:
      status: 200
    retry:
      attempts: 3
      backoffMs: 1000
      on: [502, 503, 504]
```

---

## Dynamic Variables

These generate fresh values on each execution:

| Variable | Output |
| --- | --- |
| `${$uuid}` | UUID v4 (e.g., `550e8400-e29b-41d4-a716-446655440000`) |
| `${$timestamp}` | Unix timestamp in seconds |
| `${$timestampMs}` | Unix timestamp in milliseconds |
| `${$isoTimestamp}` | ISO 8601 datetime (`2024-01-15T10:30:00Z`) |
| `${$isoDate}` | ISO 8601 date (`2024-01-15`) |
| `${$randomInt}` | Random integer 0–1000 |
| `${$randomInt(1,100)}` | Random integer in custom range |

---

## Variable Resolution Order

Variables are resolved in this priority (highest wins):

1. **CLI overrides**: `--var KEY=VAL`
2. **Runtime captures**: Values captured during step execution
3. **Step-level `vars:`**: Variables defined in the current step
4. **File-level `vars:`**: Variables defined at the top of the YAML file
5. **Global `paynal.env`**: Key-value pairs from `paynal.env` and `.env` files
6. **System environment**: `std::env::vars()`

---

## MCP Server Integration

Paynal exposes a Model Context Protocol server via `paynal mcp` that communicates
over stdio using JSON-RPC 2.0. Available tools:

| Tool | Description |
| --- | --- |
| `list_routines` | List all YAML files in the workspace `collections/` directory |
| `execute_routine` | Execute a request/routine by relative path (e.g., `auth/login`) |
| `create_routine` | Create a new YAML file with provided content |

### Connecting via MCP Config

To connect an AI agent to Paynal's MCP server, add this to your `mcp_config.json`:

```json
{
  "mcpServers": {
    "paynal": {
      "command": "paynal",
      "args": ["mcp"]
    }
  }
}
```

> [!IMPORTANT]
> The `paynal` binary must be in the system PATH, or use the full absolute path
> to the binary in the `command` field.

---

## Environment Files

- **`paynal.env`** — Project-level variables (committed to Git for shared config)
- **`.env`** — Local secrets (add to `.gitignore`)

Format:
```env
BASE_URL=https://api.staging.example.com
API_KEY=sk-secret-key-here
EMAIL=test@example.com
```

---

## Workflow: Creating and Running an API Test

1. **Initialize** (if no `paynal.json` exists):
   ```bash
   paynal init
   ```

2. **Create a request** using the template generator:
   ```bash
   paynal add auth/login
   ```
   Then edit `collections/auth/login.yaml` with the actual endpoint details.

3. **Test with dry-run** first:
   ```bash
   paynal exec auth/login --dry-run
   ```

4. **Execute**:
   ```bash
   paynal exec auth/login --verbose
   ```

5. **Run entire folder** (parallel execution):
   ```bash
   paynal exec auth/
   ```

6. **Generate documentation**:
   ```bash
   paynal doc collections/ --out docs/api.md
   ```

---

## Import From Other Tools

```bash
# Postman Collection v2.1
paynal import postman_collection.json

# Insomnia Export v4
paynal import insomnia_export.json

# Bruno collection directory
paynal import ./bruno-collection/

# Single Bruno file
paynal import request.bru
```

Variables are automatically translated: `{{var}}` → `${var}`.

---

## Tips for AI Agents

1. **Always use `--dry-run` first** when creating new requests to validate the
   YAML structure before sending actual HTTP requests.
2. **Use `--reporter json`** to get machine-parseable output for programmatic
   analysis of test results.
3. **Chain with captures**: Extract tokens, IDs, and session data from responses
   and propagate them to subsequent steps using `capture:` blocks.
4. **Use `--fail-fast --strict-vars`** in CI pipelines to catch issues early.
5. **Use `--var` overrides** to inject environment-specific values without
   modifying YAML files.
