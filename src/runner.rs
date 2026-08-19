use crate::evaluator::VariableContext;
use crate::models::{AssertSpec, MatchSpec, RequestSpec};
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
            // 1. Status code assertion
            if let Some(expected_status) = asserts.status {
                if status == expected_status {
                    assert_messages.push(format!("Status code {} == {}", status, expected_status));
                } else {
                    asserts_passed = false;
                    assert_messages.push(format!("Status code mismatch: got {}, expected {}", status, expected_status));
                }
            }

            // 2. Max duration (latency) assertion
            if let Some(max_ms) = asserts.max_duration_ms {
                if duration_ms <= max_ms {
                    assert_messages.push(format!("Latency {} ms <= {} ms", duration_ms, max_ms));
                } else {
                    asserts_passed = false;
                    assert_messages.push(format!("Latency {} ms exceeded maximum {} ms", duration_ms, max_ms));
                }
            }

            // 3. Response Headers assertion
            if let Some(expected_headers) = &asserts.headers {
                for (k, v) in expected_headers {
                    let eval_key = ctx.interpolate(k);
                    let eval_val = ctx.interpolate(v);

                    let found_header = headers.iter().find(|(h_name, _)| h_name.as_str().eq_ignore_ascii_case(&eval_key));
                    match found_header {
                        Some((_, val)) => {
                            let actual_val = val.to_str().unwrap_or("");
                            if actual_val == eval_val {
                                assert_messages.push(format!("Header '{}' == '{}'", eval_key, eval_val));
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("Header '{}' mismatch: got '{}', expected '{}'", eval_key, actual_val, eval_val));
                            }
                        }
                        None => {
                            asserts_passed = false;
                            assert_messages.push(format!("Header '{}' not found in response", eval_key));
                        }
                    }
                }
            }

            // 4. JSON value assertions (JSONPath)
            if let Some(json_asserts) = &asserts.json {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (jsonpath_expr, expected_val) in json_asserts {
                        let norm_expr = if !jsonpath_expr.starts_with('$') {
                            format!("$.{}", jsonpath_expr)
                        } else {
                            jsonpath_expr.clone()
                        };
                        let eval_expected = match expected_val {
                            serde_json::Value::String(s) => serde_json::Value::String(ctx.interpolate(s)),
                            other => other.clone(),
                        };

                        if let Ok(results) = jsonpath_lib::select(&json_val, &norm_expr) {
                            if let Some(first) = results.first() {
                                if *first == &eval_expected {
                                    assert_messages.push(format!("JSONPath {} == {:?}", norm_expr, eval_expected));
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} mismatch: got {:?}, expected {:?}", norm_expr, first, eval_expected));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found in response", norm_expr));
                            }
                        } else {
                            asserts_passed = false;
                            assert_messages.push(format!("Invalid JSONPath expression: {}", norm_expr));
                        }
                    }
                } else {
                    asserts_passed = false;
                    assert_messages.push("Response body is not valid JSON for json value assertions".to_string());
                }
            }

            // 5. JSON property existence assertion (exists / present)
            if let Some(exists_spec) = &asserts.exists {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for expr in exists_spec.as_slice() {
                        let eval_expr = ctx.interpolate(expr);
                        let norm_expr = if !eval_expr.starts_with('$') {
                            format!("$.{}", eval_expr)
                        } else {
                            eval_expr
                        };

                        match jsonpath_lib::select(&json_val, &norm_expr) {
                            Ok(results) if !results.is_empty() => {
                                assert_messages.push(format!("Property '{}' exists in response JSON", norm_expr));
                            }
                            _ => {
                                asserts_passed = false;
                                assert_messages.push(format!("Property '{}' was NOT found in response JSON", norm_expr));
                            }
                        }
                    }
                } else {
                    asserts_passed = false;
                    assert_messages.push("Response body is not valid JSON for exists assertion".to_string());
                }
            }

            // 6. JSON property absence assertion (not_exists / missing)
            if let Some(not_exists_spec) = &asserts.not_exists {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for expr in not_exists_spec.as_slice() {
                        let eval_expr = ctx.interpolate(expr);
                        let norm_expr = if !eval_expr.starts_with('$') {
                            format!("$.{}", eval_expr)
                        } else {
                            eval_expr
                        };

                        match jsonpath_lib::select(&json_val, &norm_expr) {
                            Ok(results) if results.is_empty() => {
                                assert_messages.push(format!("Property '{}' is absent in response JSON (as expected)", norm_expr));
                            }
                            Ok(results) => {
                                asserts_passed = false;
                                assert_messages.push(format!("Property '{}' is present in response JSON ({:?}), but expected to be absent", norm_expr, results.first()));
                            }
                            Err(_) => {
                                assert_messages.push(format!("Property '{}' is absent in response JSON (as expected)", norm_expr));
                            }
                        }
                    }
                } else {
                    assert_messages.push("Response body is non-JSON, property absent (as expected)".to_string());
                }
            }

            // 7. Substring contains assertion (body or per-property)
            if let Some(contains_spec) = &asserts.contains {
                match contains_spec {
                    MatchSpec::Single(substr) => {
                        let eval_sub = ctx.interpolate(substr);
                        if body.contains(&eval_sub) {
                            assert_messages.push(format!("Body contains '{}'", eval_sub));
                        } else {
                            asserts_passed = false;
                            assert_messages.push(format!("Body does NOT contain expected substring '{}'", eval_sub));
                        }
                    }
                    MatchSpec::List(list) => {
                        for substr in list {
                            let eval_sub = ctx.interpolate(substr);
                            if body.contains(&eval_sub) {
                                assert_messages.push(format!("Body contains '{}'", eval_sub));
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("Body does NOT contain expected substring '{}'", eval_sub));
                            }
                        }
                    }
                    MatchSpec::Map(map) => {
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                            for (path_expr, expected_sub) in map {
                                let eval_sub = ctx.interpolate(expected_sub);
                                let norm_expr = if !path_expr.starts_with('$') {
                                    format!("$.{}", path_expr)
                                } else {
                                    path_expr.clone()
                                };

                                match jsonpath_lib::select(&json_val, &norm_expr) {
                                    Ok(results) if !results.is_empty() => {
                                        let mut matched_any = false;
                                        for r in &results {
                                            let val_str = match r {
                                                serde_json::Value::String(s) => s.clone(),
                                                other => other.to_string(),
                                            };
                                            if val_str.contains(&eval_sub) {
                                                matched_any = true;
                                                assert_messages.push(format!("Property '{}' (\"{}\") contains '{}'", norm_expr, val_str, eval_sub));
                                                break;
                                            }
                                        }
                                        if !matched_any {
                                            asserts_passed = false;
                                            let first_val = results.first().map(|v| match v { serde_json::Value::String(s) => s.clone(), other => other.to_string() }).unwrap_or_default();
                                            assert_messages.push(format!("Property '{}' (\"{}\") does NOT contain '{}'", norm_expr, first_val, eval_sub));
                                        }
                                    }
                                    _ => {
                                        asserts_passed = false;
                                        assert_messages.push(format!("Property '{}' not found in response JSON for contains check", norm_expr));
                                    }
                                }
                            }
                        } else {
                            asserts_passed = false;
                            assert_messages.push("Response body is not valid JSON for property contains assertion".to_string());
                        }
                    }
                }
            }

            // 8. Case-insensitive substring contains assertion (icontains, body or per-property)
            if let Some(icontains_spec) = &asserts.icontains {
                let lower_body = body.to_lowercase();
                match icontains_spec {
                    MatchSpec::Single(substr) => {
                        let eval_sub = ctx.interpolate(substr);
                        if lower_body.contains(&eval_sub.to_lowercase()) {
                            assert_messages.push(format!("Body contains (case-insensitive) '{}'", eval_sub));
                        } else {
                            asserts_passed = false;
                            assert_messages.push(format!("Body does NOT contain (case-insensitive) '{}'", eval_sub));
                        }
                    }
                    MatchSpec::List(list) => {
                        for substr in list {
                            let eval_sub = ctx.interpolate(substr);
                            if lower_body.contains(&eval_sub.to_lowercase()) {
                                assert_messages.push(format!("Body contains (case-insensitive) '{}'", eval_sub));
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("Body does NOT contain (case-insensitive) '{}'", eval_sub));
                            }
                        }
                    }
                    MatchSpec::Map(map) => {
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                            for (path_expr, expected_sub) in map {
                                let eval_sub = ctx.interpolate(expected_sub);
                                let norm_expr = if !path_expr.starts_with('$') {
                                    format!("$.{}", path_expr)
                                } else {
                                    path_expr.clone()
                                };

                                match jsonpath_lib::select(&json_val, &norm_expr) {
                                    Ok(results) if !results.is_empty() => {
                                        let mut matched_any = false;
                                        for r in &results {
                                            let val_str = match r {
                                                serde_json::Value::String(s) => s.clone(),
                                                other => other.to_string(),
                                            };
                                            if val_str.to_lowercase().contains(&eval_sub.to_lowercase()) {
                                                matched_any = true;
                                                assert_messages.push(format!("Property '{}' (\"{}\") contains (case-insensitive) '{}'", norm_expr, val_str, eval_sub));
                                                break;
                                            }
                                        }
                                        if !matched_any {
                                            asserts_passed = false;
                                            let first_val = results.first().map(|v| match v { serde_json::Value::String(s) => s.clone(), other => other.to_string() }).unwrap_or_default();
                                            assert_messages.push(format!("Property '{}' (\"{}\") does NOT contain (case-insensitive) '{}'", norm_expr, first_val, eval_sub));
                                        }
                                    }
                                    _ => {
                                        asserts_passed = false;
                                        assert_messages.push(format!("Property '{}' not found in response JSON for icontains check", norm_expr));
                                    }
                                }
                            }
                        } else {
                            asserts_passed = false;
                            assert_messages.push("Response body is not valid JSON for property icontains assertion".to_string());
                        }
                    }
                }
            }

            // 9. Forbidden substring assertion (not_contains, body or per-property)
            if let Some(not_contains_spec) = &asserts.not_contains {
                match not_contains_spec {
                    MatchSpec::Single(substr) => {
                        let eval_sub = ctx.interpolate(substr);
                        if !body.contains(&eval_sub) {
                            assert_messages.push(format!("Body does not contain forbidden '{}'", eval_sub));
                        } else {
                            asserts_passed = false;
                            assert_messages.push(format!("Body contains forbidden substring '{}'", eval_sub));
                        }
                    }
                    MatchSpec::List(list) => {
                        for substr in list {
                            let eval_sub = ctx.interpolate(substr);
                            if !body.contains(&eval_sub) {
                                assert_messages.push(format!("Body does not contain forbidden '{}'", eval_sub));
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("Body contains forbidden substring '{}'", eval_sub));
                            }
                        }
                    }
                    MatchSpec::Map(map) => {
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                            for (path_expr, forbidden_sub) in map {
                                let eval_sub = ctx.interpolate(forbidden_sub);
                                let norm_expr = if !path_expr.starts_with('$') {
                                    format!("$.{}", path_expr)
                                } else {
                                    path_expr.clone()
                                };

                                match jsonpath_lib::select(&json_val, &norm_expr) {
                                    Ok(results) if !results.is_empty() => {
                                        let mut found_forbidden = false;
                                        for r in &results {
                                            let val_str = match r {
                                                serde_json::Value::String(s) => s.clone(),
                                                other => other.to_string(),
                                            };
                                            if val_str.contains(&eval_sub) {
                                                found_forbidden = true;
                                                asserts_passed = false;
                                                assert_messages.push(format!("Property '{}' (\"{}\") contains forbidden '{}'", norm_expr, val_str, eval_sub));
                                                break;
                                            }
                                        }
                                        if !found_forbidden {
                                            let first_val = results.first().map(|v| match v { serde_json::Value::String(s) => s.clone(), other => other.to_string() }).unwrap_or_default();
                                            assert_messages.push(format!("Property '{}' (\"{}\") does not contain forbidden '{}' (as expected)", norm_expr, first_val, eval_sub));
                                        }
                                    }
                                    _ => {
                                        assert_messages.push(format!("Property '{}' is absent, forbidden substring not present (as expected)", norm_expr));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 10. Case-insensitive forbidden substring assertion (not_icontains, body or per-property)
            if let Some(not_icontains_spec) = &asserts.not_icontains {
                let lower_body = body.to_lowercase();
                match not_icontains_spec {
                    MatchSpec::Single(substr) => {
                        let eval_sub = ctx.interpolate(substr);
                        if !lower_body.contains(&eval_sub.to_lowercase()) {
                            assert_messages.push(format!("Body does not contain forbidden (case-insensitive) '{}'", eval_sub));
                        } else {
                            asserts_passed = false;
                            assert_messages.push(format!("Body contains forbidden (case-insensitive) '{}'", eval_sub));
                        }
                    }
                    MatchSpec::List(list) => {
                        for substr in list {
                            let eval_sub = ctx.interpolate(substr);
                            if !lower_body.contains(&eval_sub.to_lowercase()) {
                                assert_messages.push(format!("Body does not contain forbidden (case-insensitive) '{}'", eval_sub));
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("Body contains forbidden (case-insensitive) '{}'", eval_sub));
                            }
                        }
                    }
                    MatchSpec::Map(map) => {
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                            for (path_expr, forbidden_sub) in map {
                                let eval_sub = ctx.interpolate(forbidden_sub);
                                let norm_expr = if !path_expr.starts_with('$') {
                                    format!("$.{}", path_expr)
                                } else {
                                    path_expr.clone()
                                };

                                match jsonpath_lib::select(&json_val, &norm_expr) {
                                    Ok(results) if !results.is_empty() => {
                                        let mut found_forbidden = false;
                                        for r in &results {
                                            let val_str = match r {
                                                serde_json::Value::String(s) => s.clone(),
                                                other => other.to_string(),
                                            };
                                            if val_str.to_lowercase().contains(&eval_sub.to_lowercase()) {
                                                found_forbidden = true;
                                                asserts_passed = false;
                                                assert_messages.push(format!("Property '{}' (\"{}\") contains forbidden (case-insensitive) '{}'", norm_expr, val_str, eval_sub));
                                                break;
                                            }
                                        }
                                        if !found_forbidden {
                                            let first_val = results.first().map(|v| match v { serde_json::Value::String(s) => s.clone(), other => other.to_string() }).unwrap_or_default();
                                            assert_messages.push(format!("Property '{}' (\"{}\") does not contain forbidden (case-insensitive) '{}' (as expected)", norm_expr, first_val, eval_sub));
                                        }
                                    }
                                    _ => {
                                        assert_messages.push(format!("Property '{}' is absent, forbidden substring not present (as expected)", norm_expr));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 9. Regex pattern matching assertion (body or per-property)
            if let Some(regex_spec) = &asserts.regex {
                match regex_spec {
                    MatchSpec::Single(pattern) => {
                        let eval_pat = ctx.interpolate(pattern);
                        match regex::Regex::new(&eval_pat) {
                            Ok(re) => {
                                if re.is_match(&body) {
                                    assert_messages.push(format!("Body matches regex '{}'", eval_pat));
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("Body does NOT match regex pattern '{}'", eval_pat));
                                }
                            }
                            Err(e) => {
                                asserts_passed = false;
                                assert_messages.push(format!("Invalid regex pattern '{}': {}", eval_pat, e));
                            }
                        }
                    }
                    MatchSpec::List(list) => {
                        for pattern in list {
                            let eval_pat = ctx.interpolate(pattern);
                            match regex::Regex::new(&eval_pat) {
                                Ok(re) => {
                                    if re.is_match(&body) {
                                        assert_messages.push(format!("Body matches regex '{}'", eval_pat));
                                    } else {
                                        asserts_passed = false;
                                        assert_messages.push(format!("Body does NOT match regex pattern '{}'", eval_pat));
                                    }
                                }
                                Err(e) => {
                                    asserts_passed = false;
                                    assert_messages.push(format!("Invalid regex pattern '{}': {}", eval_pat, e));
                                }
                            }
                        }
                    }
                    MatchSpec::Map(map) => {
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                            for (path_expr, pattern) in map {
                                let eval_pat = ctx.interpolate(pattern);
                                let norm_expr = if !path_expr.starts_with('$') {
                                    format!("$.{}", path_expr)
                                } else {
                                    path_expr.clone()
                                };

                                match regex::Regex::new(&eval_pat) {
                                    Ok(re) => {
                                        match jsonpath_lib::select(&json_val, &norm_expr) {
                                            Ok(results) if !results.is_empty() => {
                                                let mut matched_any = false;
                                                for r in &results {
                                                    let val_str = match r {
                                                        serde_json::Value::String(s) => s.clone(),
                                                        other => other.to_string(),
                                                    };
                                                    if re.is_match(&val_str) {
                                                        matched_any = true;
                                                        assert_messages.push(format!("Property '{}' (\"{}\") matches regex '{}'", norm_expr, val_str, eval_pat));
                                                        break;
                                                    }
                                                }
                                                if !matched_any {
                                                    asserts_passed = false;
                                                    let first_val = results.first().map(|v| match v { serde_json::Value::String(s) => s.clone(), other => other.to_string() }).unwrap_or_default();
                                                    assert_messages.push(format!("Property '{}' (\"{}\") does NOT match regex '{}'", norm_expr, first_val, eval_pat));
                                                }
                                            }
                                            _ => {
                                                asserts_passed = false;
                                                assert_messages.push(format!("Property '{}' not found in response JSON for regex check", norm_expr));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        asserts_passed = false;
                                        assert_messages.push(format!("Invalid regex pattern '{}': {}", eval_pat, e));
                                    }
                                }
                            }
                        } else {
                            asserts_passed = false;
                            assert_messages.push("Response body is not valid JSON for property regex assertion".to_string());
                        }
                    }
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
