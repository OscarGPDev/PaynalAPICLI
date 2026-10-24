# Global Multi-Workspace Directory & MCP Server Reference

---

## 1. Global Workspace Registry (`paynal global`)

Paynal maintains a central registry of workspaces on the host machine so agents and developers can discover, inspect, and execute API requests across multiple microservice repositories without manually changing directories.

### Managing & Querying Registered Workspaces

```bash
# Register the current directory into the global registry
paynal global add -d "Payments & Billing Core API"

# Register a specific path with an explicit workspace handle
paynal global add auth --path ../auth-service -d "Identity & Access Management"

# List all registered workspaces (use --json for structured agent parsing)
paynal global list
paynal global list --json

# Search and discover routes/routines across ALL registered workspaces
paynal global discover
paynal global discover login
paynal global discover login --json

# Read/inspect the raw YAML specification of a route in another workspace
paynal global show @auth/v1/login

# Remove a workspace from the global registry
paynal global remove auth
```

### Cross-Workspace Execution (`@workspace` & `-C`)

There are two ways to run commands targeting another registered workspace:

1. **`@<workspace>/<path>` Handle Syntax**:
   Automatically resolves the target workspace's `paynal.json`, `paynal.env`, and `.env` context:
   ```bash
   # Execute a single request or routine in @auth
   paynal exec @auth/v1/login

   # Execute all requests in the @auth workspace collections folder
   paynal exec @auth
   ```

2. **`-C <workspace>` (`--workspace`) Global Flag**:
   Switches Paynal's working directory to the registered workspace before executing any subcommand:
   ```bash
   paynal -C auth exec v1/login --verbose
   paynal -C auth add v2/refresh --method POST
   paynal -C auth req POST "${BASE_URL}/health"
   ```

---

## 2. Model Context Protocol Server (`paynal mcp`)

Paynal includes a built-in MCP server over `stdio` (JSON-RPC 2.0) launched via `paynal mcp`.

### MCP Configuration (`mcp_config.json`)

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

### Exposed MCP Tools (6 Tools)

| Tool Name | Required Args | Optional Args | Description |
| --- | --- | --- | --- |
| `list_routines` | *(none)* | *(none)* | Lists all `.yaml`/`.yml` files inside the active workspace's `collections/` directory. |
| `execute_routine` | `path` (`string`) | *(none)* | Executes a request or routine by relative path (e.g. `auth/login`) or cross-workspace handle (`@auth/login`). |
| `create_routine` | `path` (`string`), `content` (`string`) | *(none)* | Writes a new request or routine YAML specification into `collections/<path>.yaml`. |
| `list_global_workspaces` | *(none)* | *(none)* | Returns all registered global workspaces with their paths, descriptions, and routine counts. |
| `discover_global_routines` | *(none)* | `query` (`string`) | Searches across all registered global workspaces by handle, name, HTTP method, URL, or description. |
| `execute_inline_request` | `url` (`string`) | `method` (`string`), `headers` (`object`), `body` (`string`), `workspace` (`string`) | Executes an ad-hoc HTTP request directly without creating a YAML file, optionally loading environment variables from a registered `workspace`. |
