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

---

## 📦 Installation & Usage

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

# Execute all routines in a collection folder concurrently
paynal exec auth --parallel

# Export execution output log
paynal exec auth/login --export ./logs/login_res.txt
```

---

## 📄 Routine Specification (YAML)

Paynal routines allow chaining outputs from one step into inputs for another:

```yaml
version: "1"
name: "User Authentication & Profile Routine"
description: "Authenticates a user, captures header token, and fetches profile"
vars:
  baseUrl: "https://api.example.com/v1"

steps:
  - id: "login"
    name: "User Login"
    request:
      method: "POST"
      url: "${baseUrl}/auth/login"
      headers:
        Content-Type: "application/json"
      body: |
        {
          "email": "${ENV_USER_EMAIL}",
          "password": "${ENV_USER_PASSWORD}"
        }
    capture:
      authToken: "header.Authorization"    # Capture from Response Header
      userId: "$.data.user.id"             # Capture from Response JSON Body
    assert:
      status: 200

  - id: "get-profile"
    name: "Fetch Profile"
    request:
      method: "GET"
      url: "${baseUrl}/users/${userId}"
      headers:
        Authorization: "${authToken}"
    assert:
      status: 200
```

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

## 🛠️ Commands Overview

| Command | Description |
| :--- | :--- |
| `paynal init` | Initializes `paynal.json` manifest and directory structure. |
| `paynal add <path>` | Creates a single request or routine template (`--routine`, `--type`, `-b`/`--body`). |
| `paynal exec <path>` | Runs a request, routine, or folder (`--parallel`, `--export`, `--threads`). |
| `paynal remove <path>` | Unlinks or permanently deletes files (`--clean`). |
| `paynal clean <target>` | Sweeps output directory or orphan workspace files (`project` \| `out`). |
| `paynal doc <path>` | Generates human-readable Markdown docs (`--IO "field:type:desc"`). |
| `paynal export <path>` | Exports to `curl`, `postman`, or `insomnia` formats (`--type`). |
| `paynal import <file>` | Imports requests from `postman` v2.1 or `insomnia` v4 JSON files (`--out`). |
| `paynal ui` \| `tui` | Launches interactive Terminal User Interface (TUI) dashboard (`-E env`). |
| `paynal mcp` | Starts Model Context Protocol stdio server for AI agents. |

---

## 🤝 Contributing & License

Paynalapicli is open-source under the [GNU General Public License v3.0 (GPL-3.0)](LICENSE). Contributions, bug reports, and feature requests are welcome!
