# Paynal YAML Schema, Chaining & Assertions Reference

> [!IMPORTANT]
> Paynal enforces strict schema validation (`#[serde(deny_unknown_fields)]`). Every YAML file in `collections/` **must** include `version: "1.0"` and `name: "..."` at the root, and must not contain unknown keys (such as `type: routine`).

---

## 1. Single Request Format (`collections/<path>.yaml`)

Use this format for standalone endpoint tests. Note that `params`, `auth`, `retry`, `timeout_ms`, and `form_data` belong **inside** the `request:` block, and `body` is a YAML string (use a literal block `|` for multiline JSON/XML).

```yaml
version: "1.0"
name: "Login Request"
description: "Authenticate user and verify token structure"

vars:
  baseUrl: "https://api.example.com"

output_sections: ["status", "asserts", "body"]

request:
  method: "POST"
  url: "${baseUrl}/api/auth/login"
  params:
    client: "web"
    locale: "en-US"
  headers:
    Content-Type: "application/json"
  body: |
    {
      "email": "${EMAIL}",
      "password": "${PASSWORD}"
    }
  timeout_ms: 10000
  follow_redirects: true
  retry:
    attempts: 3
    backoff_ms: 500
    on: [502, 503, 504]

assert:
  status: 200
  max_duration_ms: 2000
  header_contains:
    Content-Type: "application/json"
  json:
    "$.success": true
    "$.data.role": "admin"
  exists:
    - "$.data.accessToken"
```

### Authentication (`request.auth`)

Place `auth:` inside `request:`. The `type` field **must be lowercase** (`bearer`, `basic`, or `apikey`):

```yaml
# Bearer Token (sets Authorization: Bearer <token>)
request:
  method: "GET"
  url: "${baseUrl}/profile"
  auth:
    type: bearer
    token: "${ACCESS_TOKEN}"
```

```yaml
# Basic Auth (sets Authorization: Basic <base64>)
request:
  method: "GET"
  url: "${baseUrl}/admin"
  auth:
    type: basic
    username: "${ADMIN_USER}"
    password: "${ADMIN_PASS}"
```

```yaml
# API Key (in "header" or "query")
request:
  method: "GET"
  url: "${baseUrl}/items"
  auth:
    type: apikey
    key: "X-API-Key"
    value: "${API_KEY}"
    in: "header" # or "query"
```

### Form Data (`request.form_data`)

For `application/x-www-form-urlencoded` or multipart form fields, use `form_data` inside `request:`:

```yaml
request:
  method: "POST"
  url: "${baseUrl}/oauth/token"
  form_data:
    grant_type: "client_credentials"
    client_id: "${CLIENT_ID}"
```

---

## 2. Multi-Step Routine Format (`steps:`)

A YAML file becomes a multi-step routine when it defines a non-empty `steps:` list. Variables extracted via `capture:` in one step are automatically propagated to subsequent steps.

> [!NOTE]
> `capture:` is a property of each step (`steps[].capture`), not of a single-request root file.

```yaml
version: "1.0"
name: "User Lifecycle Flow"
description: "Login, fetch profile, and update display name"
continue_on_failure: false

vars:
  baseUrl: "https://api.example.com"
  requestId: "${$uuid}"

steps:
  - id: "login"
    name: "Authenticate and extract token"
    vars:
      loginSource: "cli-test"
    request:
      method: "POST"
      url: "${baseUrl}/auth/login"
      headers:
        Content-Type: "application/json"
        X-Request-Id: "${requestId}"
      body: '{"email": "${EMAIL}", "password": "${PASSWORD}"}'
    capture:
      token: "$.data.token"
      userId: "$.data.userId"
      sessionHeader: "header.X-Session-Id"
      loginStatus: "status"
      loginLatency: "duration"
    assert:
      status: 200
      exists: "$.data.token"

  - id: "get_profile"
    name: "Verify current user profile"
    request:
      method: "GET"
      url: "${baseUrl}/users/${userId}"
      auth:
        type: bearer
        token: "${token}"
    assert:
      status: 200
      json:
        "$.data.email": "${EMAIL}"

  - id: "update_profile"
    name: "Update display name with retry"
    continue_on_failure: true
    retry:
      attempts: 2
      backoff_ms: 1000
      on: [500, 502, 503, 504]
    request:
      method: "PUT"
      url: "${baseUrl}/users/${userId}"
      auth:
        type: bearer
        token: "${token}"
      headers:
        Content-Type: "application/json"
      body: '{"displayName": "Agent_${$randomInt(100,999)}"}'
    assert:
      status_in: [200, 204]
```

---

## 3. Variable Capture Expressions (`steps[].capture`)

| Expression | Extracted Value | Example |
| --- | --- | --- |
| `$.path.to.field` | JSON value via JSONPath | `token: "$.data.accessToken"` |
| `header.<Header-Name>` | Response header (case-insensitive) | `sessionId: "header.X-Session-Id"` |
| `status` | HTTP status code as string | `code: "status"` |
| `duration` or `latency` | Response time in milliseconds | `ms: "duration"` |
| `body` | Raw response body string | `raw: "body"` |
| `regex:<pattern>` | First regex capture group (or full match) from body | `userId: 'regex:"id":\s*(\d+)'` |

---

## 4. Complete Assertions Reference (`assert:`)

| Field (and supported aliases) | Type | Description & Example |
| --- | --- | --- |
| `status` | `u16` | Exact HTTP status code: `status: 200` |
| `status_range` (`statusRange`) | `String` | Range or class string: `status_range: "200-299"` or `status_range: "2xx"` |
| `status_in` (`statusIn`) | `Vec<u16>` | Allowed status list: `status_in: [200, 201, 204]` |
| `max_duration_ms` (`latencyMs`) | `u128` | Maximum allowed latency in ms: `max_duration_ms: 1500` |
| `headers` | `Map<String, String>` | Exact header match (case-insensitive key) |
| `header_contains` (`headerContains`) | `Map<String, String>` | Substring match on response header values |
| `header_regex` (`headerRegex`) | `Map<String, String>` | Regex match on response header values |
| `json` | `Map<String, Value>` | Exact JSONPath equality (`"$.ok": true`, `"$.count": 5`) |
| `exists` (`present`) | `String` or `Vec<String>` | JSONPath property exists (`exists: ["$.id", "$.token"]`) |
| `not_exists` (`missing`) | `String` or `Vec<String>` | JSONPath property is absent (`not_exists: "$.error"`) |
| `contains` (`bodyContains`) | `String`, `Vec`, or `Map` | Substring in raw body or per-JSONPath property |
| `icontains` | `String`, `Vec`, or `Map` | Case-insensitive substring match |
| `not_contains` (`notContains`) | `String`, `Vec`, or `Map` | Substring must NOT appear in raw body or JSONPath property |
| `not_icontains` | `String`, `Vec`, or `Map` | Case-insensitive negative substring match |
| `regex` | `String`, `Vec`, or `Map` | Regex match on raw body or per-JSONPath property |
| `json_gt` (`gt`) | `Map<String, f64>` | Numeric `>` comparison: `"$.total": 0` |
| `json_gte` (`gte`) | `Map<String, f64>` | Numeric `>=` comparison: `"$.total": 1` |
| `json_lt` (`lt`) | `Map<String, f64>` | Numeric `<` comparison: `"$.errors": 5` |
| `json_lte` (`lte`) | `Map<String, f64>` | Numeric `<=` comparison: `"$.score": 100.0` |
| `json_length` (`length`) | `Map<String, usize>` | Length of JSON array, string, or object: `"$.items": 10` |
| `json_type` (`type`) | `Map<String, String>` | JSON type (`array`, `object`, `string`, `number`, `boolean`, `null`) |
| `schema` | `String` | Inline JSON Schema string (`'{ "type": "object", ... }'`) OR path to `.json` schema file |

---

## 5. Variables & Output Sections

### Dynamic Variables
Generated fresh on every interpolation call:
- `${$uuid}` — UUID v4
- `${$timestamp}` — Unix timestamp (seconds)
- `${$timestampMs}` — Unix timestamp (milliseconds)
- `${$isoTimestamp}` / `${$isoDate}` — RFC 3339 / ISO 8601 UTC timestamp
- `${$randomInt}` — Random integer `1..=1000`
- `${$randomInt(min,max)}` — Random integer in `min..=max`

### Variable Resolution Priority (Highest to Lowest)
1. **CLI `--var KEY=VAL`**
2. **Runtime `capture:` values** from previous steps
3. **Step-level `vars:`** (`steps[].vars`)
4. **File-level `vars:`** (root `vars:`)
5. **Profile env file** (`paynal.env.<profile>` or `.env.<profile>` when `-E <profile>` is used)
6. **Workspace env files** (`paynal.env` overrides `.env`)
7. **System environment variables** (`std::env::vars()`)

### Selective Output Sections (`output_sections` / `outputSections`)
Can be set in `paynal.json`, at YAML file root, on a routine step, on `request:`, or via `paynal req --sections`:
- `status` (aliases: `summary`, `meta`, `duration`, `latency`)
- `payload` (aliases: `request`, `req`, `sent`)
- `headers` (aliases: `response_headers`, `res_headers`)
- `asserts` (aliases: `assertions`, `assert`, `tests`)
- `body` (aliases: `response_body`, `res_body`, `response`)
- `all` (aliases: `*`, `full` — forced when `--verbose` is passed)
