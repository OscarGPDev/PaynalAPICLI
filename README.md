# Paynalapicli (`paynal`)

> **Lightning-fast, CLI-first API client, routine runner, and AI agent testing engine built in Rust.**

[![Built with Rust](https://img.shields.io/badge/Built_with-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: GPL v3](https://img.shields.io/badge/License-GPL_v3-blue.svg)](LICENSE)

---

## ⚡ The Origin & Story Behind Paynal

In Aztec mythology, **Paynal** was the swift messenger god who ran ahead to clear paths and complete rapid circuits. **Paynalapicli** combines this concept of rapid circuit execution with **API** testing in a lightweight **CLI**.

### Why Paynal Was Created
Modern API clients (like Postman and Insomnia) have transformed from simple testing tools into bloated, resource-heavy Electron applications. 

Falling back to `cURL` handled one-off requests, but `cURL` lacks native persistence, environment variable management, response data extraction, and circuit validation across sequential requests without complex scripting sorcery—even for a simple two-step circuit.

Furthermore, popular API clients increasingly lock basic collaboration features behind mandatory cloud subscriptions, making offline developers feel like second-class users. This is not a demo and I won't ask for a tithe 😉 (though donations are always welcome 🫠).

**Paynalapicli** was born out of necessity:
- **No heavy Electron runtime** — Lightweight, ultra-fast, single binary compiled in Rust.
- **Git-Native Collaboration** — Your collections are simple, human-readable YAML files stored in your repository. How you share or collaborate is 100% up to you.
- **CI/CD E2E Circuit Testing** — Validate full API circuits automatically after deployment in any pipeline.
- **AI Agent Tooling (MCP)** — Native Model Context Protocol support so AI agents can inspect, generate, and run API test suites natively.

---

## ✨ Features

- 🚀 **CLI-First Speed**: Built in Rust for instant execution and minimal CPU/memory footprint.
- 📜 **YAML Routine Files**: Define single HTTP requests or multi-step, full-circuit routines in clean YAML.
- 🔗 **Dynamic Variable Chaining**: Extract response JSON values (via JSONPath) or Response Headers (e.g. `header.Authorization`) and pass them to subsequent requests.
- ⚡ **Parallel Execution Engine**: Run collection test suites concurrently using `CPUMAX` (logical CPU limit) or `FULLMAX` concurrency modes.
- 🔒 **Secrets & Environment Support**: Load secrets securely from standard `.env` and `paynal.env` files.
- 🛡️ **Certificates & Proxy Control**: Toggle SSL verification (`validateCertificates`) and route traffic through HTTP/SOCKS5 proxies.
- 📝 **Auto-Documentation Generator**: Generate clean Markdown documentation (`paynal doc`) for routines with input/output fields.
- 📦 **Multi-Format Exporters**: Export collections to `cURL` shell scripts, `Postman v2.1`, or `Insomnia` formats.
- 📁 **File & Multipart Uploads**: Send raw binary files with `@file` syntax or upload multi-part form data and attachments via `formData:`.
- 🎨 **Terminal UI Dashboard**: Launch interactive TUI (`paynal ui` or `paynal tui`) with collection search (`/`), folder expand/collapse tree (`o` / `←` / `→`), template creation (`a`), external text editor launch (`e`), local file variable editor (`v`), environment profile manager (`E`), and full collection execution (`p` for parallel / `Enter` for sequential).
- 🤖 **Native AI Agent Integration (MCP)**: Run `paynal mcp` to connect AI assistants directly via Model Context Protocol.
- 📦 **Cross-Platform & Flatpak**: Available as pre-compiled native binaries and standalone Flatpak bundles (`paynal.flatpak`).

---

## 📦 Installation & Usage

### 🚀 Quick Install

#### Option A: Flatpak (Linux)
Download the standalone `paynal.flatpak` bundle from [GitHub Releases](https://github.com/OscarGPDev/PaynalAPICLI/releases):
```bash
# Install the standalone bundle for the current user
flatpak install --user paynal.flatpak

# Launch interactive Terminal UI dashboard
flatpak run io.github.oscargpdev.Paynal

# Execute CLI commands directly
flatpak run io.github.oscargpdev.Paynal exec auth/login

# (Recommended) Add an alias in ~/.bashrc or ~/.zshrc:
alias paynal="flatpak run io.github.oscargpdev.Paynal"
```

#### Option B: Pre-Compiled Binary (Linux, macOS, Windows)
Download the archive for your architecture (`tar.gz` for Linux/macOS or `.zip` for Windows) from [GitHub Releases](https://github.com/OscarGPDev/PaynalAPICLI/releases) and move the `paynal` executable to your `PATH` (e.g. `/usr/local/bin`).

#### Option C: Build from Source
```bash
git clone https://github.com/OscarGPDev/PaynalAPICLI.git
cd PaynalAPICLI
cargo build --release
sudo cp target/release/paynal /usr/local/bin/
```

---

### Quickstart Guide

### 1. Initialize Workspace
```bash
paynal init
```
Generates `paynal.json`, `paynal.env`, `collections/`, and `output/`.

### 2. Add Requests or Multi-Step Routines
```bash
# Add a single POST request (auto-generates JSON body template by default)
paynal add auth/login --type post

# Add a request with custom body template (json, form, text, xml, none)
paynal add auth/oauth --type post --body form

# Add a multi-step routine template
paynal add auth/e2e_circuit --routine
```

### 3. Execute Requests & Routines
```bash
# Execute a single request or routine
paynal exec auth/login

# Preview request headers/body without making network calls
paynal exec auth/login --dry-run

# Show verbose request/response diagnostics (secrets masked automatically)
paynal exec auth/login --verbose

# Run with CI reporters (human, json, junit)
paynal exec auth/login --reporter junit --out ./test-results/junit.xml

# Execute all routines in a collection folder concurrently
paynal exec auth --parallel

# Override variables, timeouts, or SSL validation on the fly
paynal exec auth/login --var BASE_URL=https://staging.api.com --timeout 5000 -k
```

### Exit Codes
Paynal returns standard exit codes for deterministic CI/CD pipelines:
- `0`: All requests and assertions passed.
- `1`: One or more assertions failed.
- `2`: Network, transport, schema, or configuration error.

---

## 📄 Routine Specification (YAML)

Paynal routines allow chaining outputs from one step into inputs for another:

```yaml
version: "1"
name: "User Authentication & Profile Routine"
description: "Authenticates a user, captures header token, and fetches profile"
continueOnFailure: false             # Stop routine immediately on step failure

vars:
  baseUrl: "https://api.example.com/v1"
  traceId: "${$uuid}"                # Built-in dynamic UUID v4

steps:
  - id: "login"
    name: "User Login"
    vars:                            # Step-local variables
      loginType: "standard"
    request:
      method: "POST"
      url: "${baseUrl}/auth/login"
      params:
        source: "cli"
      headers:
        Content-Type: "application/json"
        X-Trace-Id: "${traceId}"
      body: |
        {
          "email": "${ENV_USER_EMAIL}",
          "password": "${ENV_USER_PASSWORD}"
        }
      retry:                         # Automatic retries on transient errors
        attempts: 3
        backoffMs: 500
        on: [502, 503, 504]
    capture:
      authToken: "header.Authorization"    # Capture from Response Header
      userId: "$.data.user.id"             # Capture from Response JSON Body
      latency: "$duration"                 # Capture step latency in ms
    assert:
      status: 2xx                          # Status range
      maxDuration: 1500

  - id: "get-profile"
    name: "Fetch Profile"
    request:
      method: "GET"
      url: "${baseUrl}/users/${userId}"
      auth:
        type: bearer
        token: "${authToken}"
    assert:
      status: 200
      schema: "./schemas/user_profile.json" # JSON Schema validation
```

### 🔑 Authentication Block (`auth:`)
Easily configure authentication without manual header crafting:
```yaml
# Bearer Token
auth:
  type: bearer
  token: "${TOKEN}"

# HTTP Basic Auth
auth:
  type: basic
  username: "${USERNAME}"
  password: "${PASSWORD}"

# API Key (Header or Query)
auth:
  type: apikey
  key: "X-API-Key"
  value: "${API_KEY}"
  in: header # or "query"
```

### 🎲 Dynamic Variables
Paynal provides built-in generators for runtime values:
- `${$uuid}`: Generates a random UUID v4.
- `${$timestamp}`: Unix timestamp in seconds.
- `${$timestampMs}`: Unix timestamp in milliseconds.
- `${$isoTimestamp}`: Current UTC timestamp in ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`).
- `${$isoDate}`: Current UTC date (`YYYY-MM-DD`).
- `${$randomInt}`: Random integer between 0 and 1,000,000.
- `${$randomInt(min, max)}`: Random integer within range (e.g. `${$randomInt(100, 999)}`).

### 📁 File & Multipart Uploads

Paynal supports both raw binary file streaming and multipart form uploads:

```yaml
# 1. Multipart Form Data with File Attachment
version: "1"
name: "upload_user_profile"
request:
  method: "POST"
  url: "${baseUrl}/users/profile"
  formData:
    username: "oscar"
    bio: "Developer & Creator"
    avatar: "@./assets/avatar.png"   # Attached as multipart file part
assert:
  status: 200
```

```yaml
# 2. Raw Binary Stream Upload
version: "1"
name: "upload_raw_image"
request:
  method: "POST"
  url: "${baseUrl}/media/upload"
  headers:
    Content-Type: "image/png"
  body: "@./assets/avatar.png"       # Streams raw binary bytes directly
assert:
  status: 200
```

---

## 🧪 Assertions Reference

Paynal includes a comprehensive testing and validation engine:

```yaml
assert:
  # 1. Status Code, Range, and Sets
  status: 200
  statusRange: "2xx"           # Matches 200-299 or explicit "200-204"
  statusIn: [200, 201, 204]    # Matches any in the list

  # 2. Maximum Response Latency (in milliseconds)
  maxDuration: 500

  # 3. Response Headers Validation
  headers:
    Content-Type: "application/json"
  headerContains:
    Content-Type: "utf-8"
  headerRegex:
    Content-Type: "application/(json|problem\\+json)"

  # 4. JSON Schema Validation
  schema: "./schemas/response.json"

  # 5. JSONPath Exact Value Validation
  json:
    "$.status": "success"
    "$.authenticated": true
    "$.data.user.id": "${EXPECTED_USER_ID}"

  # 6. Numeric & Array Comparisons
  jsonGt:
    "$.data.itemsCount": 0
  jsonGte:
    "$.data.score": 80.5
  jsonLt:
    "$.data.errorCount": 1
  jsonLength:
    "$.data.items": 10
  jsonType:
    "$.data.user.id": "string"   # "string", "number", "boolean", "array", "object", "null"

  # 7. JSON Property Presence & Absence
  exists:
    - "$.data.user.id"
    - "token"                  # Auto-prepends $. if omitted
  notExists:
    - "$.error"
    - "secretKey"

  # 8. Substring Matching (Full Body or Per-Property)
  contains:
    "$.user.name": "John"
  icontains:
    "$.user.name": "john"      # Case-insensitive
  notContains:
    "$.user.role": "admin"

  # 9. Regular Expression Matching
  regex:
    "$.user.email": "^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$"
```

---

## 🛠️ Commands Overview

| Command | Aliases | Description |
| :--- | :--- | :--- |
| `paynal init` | | Initializes `paynal.json` manifest, `.gitignore`, and directory structure. |
| `paynal add <path>` | | Creates a request or routine template (`-t`/`--type`, `-b`/`--body`, `--routine`). |
| `paynal exec <path>` | `run` | Runs a request, routine, or folder (`--parallel`, `--reporter`, `--out`, `--dry-run`, `--verbose`, `--fail-fast`, `--strict-vars`, `--var`, `-k`, `--proxy`, `--timeout`). |
| `paynal remove <path>` | `rm`, `del`, `delete` | Deletes files from disk (`--clean`). |
| `paynal clean <target>` | | Sweeps output directory (`out`) or orphan/temporary files from collections (`project`). |
| `paynal doc <path>` | | Generates Markdown docs with headers, params, body, auth, captures, and asserts (`--io`). |
| `paynal export <path>` | | Exports to `curl`, `postman`, or `insomnia` formats (`--type` / `--to`). |
| `paynal import <path>` | | Imports collections from Bruno (`.bru`), Postman v2.1, or Insomnia v4 (`--out`). |
| `paynal ui` | `tui` | Launches interactive Terminal User Interface (TUI) dashboard (`-E env`). |
| `paynal mcp` | | Starts Model Context Protocol stdio JSON-RPC server for AI agents. |
| `paynal man` | | Reads interactive terminal man page or exports roff files (`--out`). |

### 📖 Offline UNIX Man Pages

You can view paginated manual pages directly in your terminal:

```bash
# View main manual page
paynal man

# View manual page for a specific subcommand
paynal man exec
paynal man add

# Export .1 man pages to system man directory
sudo paynal man --out /usr/local/share/man/man1
```

---

## 🤝 Contributing & License

Paynalapicli is open-source under the [GNU General Public License v3.0 (GPL-3.0)](LICENSE). Contributions, bug reports, and feature requests are welcome!

