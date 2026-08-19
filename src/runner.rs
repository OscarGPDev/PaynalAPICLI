use crate::evaluator::VariableContext;
use crate::models::{AssertSpec, RequestSpec};
use anyhow::{Context, Result};
use colored::*;
use reqwest::{header::HeaderName, Method};
use std::path::Path;
use std::str::FromStr;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: u16,
    pub status_text: String,
    pub duration_ms: u128,
    pub headers: reqwest::header::HeaderMap,
    pub body: String,
    pub asserts_passed: bool,
    pub assert_messages: Vec<String>,
}

pub struct HttpRunner {
    client: reqwest::Client,
}

impl HttpRunner {
    pub fn new(validate_certs: bool, proxy: Option<&str>) -> Self {
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10));

        if !validate_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }

        if let Some(proxy_url) = proxy {
            if !proxy_url.trim().is_empty() {
                if let Ok(p) = reqwest::Proxy::all(proxy_url) {
                    builder = builder.proxy(p);
                }
            }
        }

        Self {
            client: builder.build().unwrap_or_default(),
        }
    }

    pub async fn execute(
        &self,
        request: &RequestSpec,
        assert_spec: Option<&AssertSpec>,
        ctx: &VariableContext,
    ) -> Result<ExecutionResult> {
        let url = ctx.interpolate(&request.url);
        let method = Method::from_str(&request.method.to_uppercase())
            .with_context(|| format!("Invalid HTTP Method: {}", request.method))?;

        let mut req_builder = self.client.request(method.clone(), &url);

        // Add headers
        for (k, v) in &request.headers {
            let key = ctx.interpolate(k);
            let val = ctx.interpolate(v);
            if let Ok(h_name) = HeaderName::from_str(&key) {
                req_builder = req_builder.header(h_name, val);
            }
        }

        // Add multipart form-data if present
        if let Some(form_fields) = &request.form_data {
            let mut form = reqwest::multipart::Form::new();
            for (k, v) in form_fields {
                let field_key = ctx.interpolate(k);
                let field_val = ctx.interpolate(v);

                if field_val.starts_with('@') {
                    let file_path_str = field_val.trim_start_matches('@').trim();
                    let file_path = Path::new(file_path_str);
                    if !file_path.exists() {
                        anyhow::bail!("Multipart file not found: {}", file_path_str);
                    }
                    let file_bytes = std::fs::read(file_path)
                        .with_context(|| format!("Failed to read file for multipart upload: {}", file_path_str))?;
                    let file_name = file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .to_string();

                    let part = reqwest::multipart::Part::bytes(file_bytes)
                        .file_name(file_name);
                    form = form.part(field_key, part);
                } else {
                    form = form.text(field_key, field_val);
                }
            }
            req_builder = req_builder.multipart(form);
        } else if let Some(body_raw) = &request.body {
            if !body_raw.is_empty() {
                let interpolated_body = ctx.interpolate(body_raw);
                if interpolated_body.starts_with('@') {
                    let file_path_str = interpolated_body.trim_start_matches('@').trim();
                    let file_path = Path::new(file_path_str);
                    if !file_path.exists() {
                        anyhow::bail!("Binary file not found for upload: {}", file_path_str);
                    }
                    let file_bytes = std::fs::read(file_path)
                        .with_context(|| format!("Failed to read binary file for upload: {}", file_path_str))?;
                    req_builder = req_builder.body(file_bytes);
                } else {
                    req_builder = req_builder.body(interpolated_body);
                }
            }
        }

        let start = Instant::now();
        let response = req_builder
            .send()
            .await
            .with_context(|| format!("HTTP Request failed for {}", url))?;
        let duration_ms = start.elapsed().as_millis();

        let status = response.status().as_u16();
        let status_text = response.status().to_string();
        let headers = response.headers().clone();
        let body = response.text().await.unwrap_or_default();

        // Evaluate assertions
        let mut asserts_passed = true;
        let mut assert_messages = Vec::new();

        if let Some(asserts) = assert_spec {
            if let Some(expected_status) = asserts.status {
                if status == expected_status {
                    assert_messages.push(format!("Status code {} == {}", status, expected_status));
                } else {
                    asserts_passed = false;
                    assert_messages.push(format!("Status code mismatch: got {}, expected {}", status, expected_status));
                }
            }

            if let Some(json_asserts) = &asserts.json {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (jsonpath_expr, expected_val) in json_asserts {
                        let eval_expected = match expected_val {
                            serde_json::Value::String(s) => serde_json::Value::String(ctx.interpolate(s)),
                            other => other.clone(),
                        };

                        if let Ok(results) = jsonpath_lib::select(&json_val, jsonpath_expr) {
                            if let Some(first) = results.first() {
                                if *first == &eval_expected {
                                    assert_messages.push(format!("JSONPath {} == {:?}", jsonpath_expr, eval_expected));
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} mismatch: got {:?}, expected {:?}", jsonpath_expr, first, eval_expected));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found in response", jsonpath_expr));
                            }
                        } else {
                            asserts_passed = false;
                            assert_messages.push(format!("Invalid JSONPath expression: {}", jsonpath_expr));
                        }
                    }
                } else {
                    asserts_passed = false;
                    assert_messages.push("Response body is not valid JSON for assertions".to_string());
                }
            }
        }

        Ok(ExecutionResult {
            status,
            status_text,
            duration_ms,
            headers,
            body,
            asserts_passed,
            assert_messages,
        })
    }

    pub fn extract_captures(
        &self,
        res: &ExecutionResult,
        captures: &std::collections::HashMap<String, String>,
        ctx: &mut VariableContext,
    ) {
        if captures.is_empty() {
            return;
        }

        for (var_name, expr) in captures {
            if expr.starts_with("header.") || expr.starts_with("headers.") {
                let header_key = expr
                    .trim_start_matches("headers.")
                    .trim_start_matches("header.");
                for (h_name, h_val) in &res.headers {
                    if h_name.as_str().eq_ignore_ascii_case(header_key) {
                        if let Ok(val_str) = h_val.to_str() {
                            ctx.set(var_name, val_str);
                        }
                    }
                }
            } else if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&res.body) {
                if let Ok(results) = jsonpath_lib::select(&json_val, expr) {
                    if let Some(first) = results.first() {
                        let val_str = match first {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        ctx.set(var_name, val_str);
                    }
                }
            }
        }
    }

    pub fn print_result(&self, name: &str, method: &str, url: &str, res: &ExecutionResult) {
        let status_colored = if res.status < 300 {
            format!("{}", res.status).green().bold()
        } else if res.status < 400 {
            format!("{}", res.status).yellow().bold()
        } else {
            format!("{}", res.status).red().bold()
        };

        println!("\n{}", "─".repeat(60).dimmed());
        println!("🚀 Request: {} [{} {}]", name.bold(), method.cyan(), url);
        println!("⏱️  Duration: {} ms | Status: {} ({}) | Headers: {}", res.duration_ms, status_colored, res.status_text, res.headers.len());
        
        if !res.assert_messages.is_empty() {
            print!("🧪 Assertions: ");
            if res.asserts_passed {
                println!("{}", "PASSED".green().bold());
            } else {
                println!("{}", "FAILED".red().bold());
            }
            for msg in &res.assert_messages {
                println!("   • {}", msg);
            }
        }

        println!("\n📋 Response Body:");
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&res.body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                println!("{}", pretty);
            } else {
                println!("{}", res.body);
            }
        } else {
            println!("{}", res.body);
        }
        println!("{}\n", "─".repeat(60).dimmed());
    }
}
