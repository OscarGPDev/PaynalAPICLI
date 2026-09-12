use paynal::cli::ExportType;
use paynal::commands::{execute_export, execute_import};
use paynal::models::{AuthSpec, PaynalFile};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_import_bruno_bru_file() {
    let temp_dir = TempDir::new().unwrap();
    let bru_content = r#"
meta {
  name: Get User Profile
  type: http
  seq: 1
}

get {
  url: {{baseUrl}}/api/v1/users/:id
  body: json
  auth: bearer
}

headers {
  Accept: application/json
  X-Custom-Header: {{customHeader}}
}

params:query {
  include_meta: true
  page: 1
}

body:json {
  {
    "query": "{{searchQuery}}"
  }
}

auth:bearer {
  token: {{authToken}}
}

assert {
  res.status: eq 200
}
"#;

    let bru_path = temp_dir.path().join("get_user.bru");
    fs::write(&bru_path, bru_content).unwrap();

    let out_dir = temp_dir.path().join("collections");
    execute_import(
        bru_path.to_str().unwrap().to_string(),
        out_dir.to_str().unwrap().to_string(),
    )
    .unwrap();

    let imported_file = out_dir.join("get_user.yaml");
    assert!(imported_file.exists(), "Expected imported YAML file to exist");

    let yaml_content = fs::read_to_string(&imported_file).unwrap();
    let paynal_file: PaynalFile = serde_yaml::from_str(&yaml_content).unwrap();

    assert_eq!(paynal_file.name, "get_user_profile");
    let req = paynal_file.request.expect("Expected request spec");
    assert_eq!(req.method, "GET");
    assert_eq!(req.url, "${baseUrl}/api/v1/users/:id");
    assert_eq!(req.headers.get("Accept").map(|s| s.as_str()), Some("application/json"));
    assert_eq!(req.headers.get("X-Custom-Header").map(|s| s.as_str()), Some("${customHeader}"));

    let params = req.params.expect("Expected query params");
    assert_eq!(params.get("include_meta").map(|s| s.as_str()), Some("true"));
    assert_eq!(params.get("page").map(|s| s.as_str()), Some("1"));

    let body = req.body.expect("Expected body");
    assert!(body.contains("${searchQuery}"));

    assert_eq!(
        req.auth,
        Some(AuthSpec::Bearer {
            token: "${authToken}".to_string()
        })
    );

    let assert_spec = paynal_file.assert.expect("Expected assert spec");
    assert_eq!(assert_spec.status, Some(200));
}

#[test]
fn test_import_postman_with_vars_and_auth() {
    let temp_dir = TempDir::new().unwrap();
    let postman_json = serde_json::json!({
        "info": {
            "name": "Sample Postman Collection",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": [
            {
                "name": "Create Item",
                "request": {
                    "method": "POST",
                    "url": {
                        "raw": "{{baseUrl}}/items?category={{cat}}",
                        "query": [
                            { "key": "category", "value": "{{cat}}" }
                        ]
                    },
                    "header": [
                        { "key": "Content-Type", "value": "application/json" }
                    ],
                    "body": {
                        "mode": "raw",
                        "raw": "{\"title\": \"{{itemTitle}}\"}"
                    },
                    "auth": {
                        "type": "bearer",
                        "bearer": [
                            { "key": "token", "value": "{{secretToken}}" }
                        ]
                    }
                }
            }
        ]
    });

    let postman_path = temp_dir.path().join("collection.json");
    fs::write(&postman_path, serde_json::to_string(&postman_json).unwrap()).unwrap();

    let out_dir = temp_dir.path().join("collections");
    execute_import(
        postman_path.to_str().unwrap().to_string(),
        out_dir.to_str().unwrap().to_string(),
    )
    .unwrap();

    let yaml_path = out_dir.join("create_item.yaml");
    assert!(yaml_path.exists());

    let yaml_content = fs::read_to_string(&yaml_path).unwrap();
    let paynal_file: PaynalFile = serde_yaml::from_str(&yaml_content).unwrap();

    let req = paynal_file.request.unwrap();
    assert_eq!(req.method, "POST");
    assert_eq!(req.url, "${baseUrl}/items?category=${cat}");
    assert_eq!(req.params.unwrap().get("category").map(|s| s.as_str()), Some("${cat}"));
    assert!(req.body.unwrap().contains("${itemTitle}"));
    assert_eq!(
        req.auth,
        Some(AuthSpec::Bearer {
            token: "${secretToken}".to_string()
        })
    );
}

#[test]
fn test_import_insomnia_with_request_groups() {
    let temp_dir = TempDir::new().unwrap();
    let insomnia_json = serde_json::json!({
        "_type": "export",
        "__export_format": 4,
        "resources": [
            {
                "_id": "fld_auth",
                "_type": "request_group",
                "name": "Authentication",
                "parentId": null
            },
            {
                "_id": "req_login",
                "_type": "request",
                "parentId": "fld_auth",
                "name": "Login Request",
                "method": "POST",
                "url": "{{ _.baseUrl }}/auth/login",
                "headers": [
                    { "name": "Content-Type", "value": "application/json" }
                ],
                "body": {
                    "text": "{\"user\": \"{{ _.username }}\"}"
                },
                "authentication": {
                    "type": "basic",
                    "username": "{{ _.adminUser }}",
                    "password": "{{ _.adminPass }}"
                }
            }
        ]
    });

    let insomnia_path = temp_dir.path().join("insomnia.json");
    fs::write(&insomnia_path, serde_json::to_string(&insomnia_json).unwrap()).unwrap();

    let out_dir = temp_dir.path().join("collections");
    execute_import(
        insomnia_path.to_str().unwrap().to_string(),
        out_dir.to_str().unwrap().to_string(),
    )
    .unwrap();

    // Verify file is placed in subfolder "authentication"
    let nested_yaml = out_dir.join("authentication").join("login_request.yaml");
    assert!(nested_yaml.exists(), "Expected nested login_request.yaml in authentication/ folder");

    let yaml_content = fs::read_to_string(&nested_yaml).unwrap();
    let paynal_file: PaynalFile = serde_yaml::from_str(&yaml_content).unwrap();

    let req = paynal_file.request.unwrap();
    assert_eq!(req.url, "${baseUrl}/auth/login");
    assert!(req.body.unwrap().contains("${username}"));
    assert_eq!(
        req.auth,
        Some(AuthSpec::Basic {
            username: "${adminUser}".to_string(),
            password: Some("${adminPass}".to_string())
        })
    );
}

#[test]
fn test_export_curl_with_quotes_params_and_auth() {
    let temp_dir = TempDir::new().unwrap();
    let mut params = std::collections::HashMap::new();
    params.insert("filter".to_string(), "active".to_string());

    let paynal_file = PaynalFile {
        version: "1".to_string(),
        name: "test_quote_curl".to_string(),
        description: None,
        vars: Default::default(),
        request: Some(paynal::models::RequestSpec {
            method: "POST".to_string(),
            url: "https://api.example.com/items".to_string(),
            params: Some(params),
            headers: Default::default(),
            body: Some("{\"note\": \"O'Reilly books\"}".to_string()),
            form_data: None,
            timeout_ms: None,
            retry: None,
            follow_redirects: None,
            auth: Some(AuthSpec::Bearer {
                token: "token123".to_string(),
            }),
        }),
        steps: vec![],
        assert: None,
        continue_on_failure: None,
    };

    let yaml_path = temp_dir.path().join("test_req.yaml");
    fs::write(&yaml_path, serde_yaml::to_string(&paynal_file).unwrap()).unwrap();

    execute_export(yaml_path.to_str().unwrap().to_string(), ExportType::Curl).unwrap();

    let curl_sh_path = temp_dir.path().join("test_req.curl.sh");
    assert!(curl_sh_path.exists());

    let script = fs::read_to_string(&curl_sh_path).unwrap();
    assert!(script.contains("filter=active"), "Expected query parameter in curl URL");
    assert!(script.contains("Authorization: Bearer token123"), "Expected auth header in curl");
    // Check single quote escaping: O'Reilly becomes O'\''Reilly
    assert!(script.contains(r"O'\''Reilly"), "Expected escaped single quote in body");
}
