# Import, Export, Documentation & CI/CD Reference

---

## 1. Importing External Collections (`paynal import`)

Paynal converts Postman, Insomnia, and Bruno collections into native Paynal YAML files under `collections/` (or a custom directory via `--out`), automatically translating `{{var}}` placeholders into `${var}`.

```bash
# Import Postman Collection v2.1 JSON
paynal import postman_collection.json

# Import Insomnia Export v4 JSON into a custom subfolder
paynal import insomnia_export.json --out collections/imported

# Import a Bruno collection folder (.bru files) or single .bru file
paynal import ./bruno-collection/
paynal import ./bruno-collection/auth/login.bru
```

---

## 2. Exporting Collections (`paynal export`)

Export a single request, routine, or entire collection folder to `curl`, `postman`, or `insomnia`. Output files are saved to the workspace `output/` directory.

```bash
# Export as executable curl command script
paynal export auth/login --type curl

# Export folder as a Postman v2.1 collection
paynal export auth --type postman

# Export as Insomnia v4 collection
paynal export auth --type insomnia
```

---

## 3. Generating Markdown Documentation (`paynal doc`)

Generates Markdown documentation inside `docs/<relative-path>.md` based on the YAML specifications in `collections/`.

```bash
# Generate docs for a single request or routine
paynal doc auth/login

# Generate docs for all files in a folder
paynal doc auth

# Include custom Input/Output field tables ("field:type:Description")
paynal doc auth/login --io "email:string:User email,token:string:JWT Bearer token"
```

---

## 4. CI/CD Execution, Parallelism & Reporters

### Machine-Readable Reporters (`--reporter`)

```bash
# JSON report printed to stdout (ideal for AI agents and jq pipelines)
paynal exec auth --reporter json

# Save JSON report to file
paynal exec auth --reporter json --out results.json

# JUnit XML report for GitHub Actions / GitLab CI / Jenkins test summaries
paynal exec collections --parallel --reporter junit --out junit-results.xml
```

### Parallel Execution & Concurrency Control

When executing a folder, pass `--parallel` to run requests concurrently:

```bash
# Parallel execution with default concurrency
paynal exec regression_suite --parallel

# Limit concurrency to available CPU cores
paynal exec regression_suite --parallel --threads CPUMAX

# Custom fixed concurrency (e.g., 8 concurrent requests)
paynal exec regression_suite --parallel --threads 8

# Strict CI pipeline mode: fail on first error and reject unresolved ${VAR}
paynal exec regression_suite --parallel --fail-fast --strict-vars -E staging
```

### Workspace Maintenance (`paynal remove` & `paynal clean`)

```bash
# Unregister request from manifest index and delete YAML file from disk
paynal remove auth/deprecated_login --clean

# Clean unindexed orphan files from collections/ or wipe output/ logs
paynal clean project
paynal clean out
```
