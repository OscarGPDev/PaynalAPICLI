use crate::evaluator::VariableContext;
use crate::models::{AssertSpec, MatchSpec, RequestSpec, RetrySpec};
use anyhow::{Context, Result};
use colored::*;
use reqwest::{header::HeaderName, Method};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::time::Instant;

use std::fs;

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
    no_redirect_client: reqwest::Client,
    pub default_timeout_ms: Option<u64>,
}

impl Default for HttpRunner {
    fn default() -> Self {
        Self::new(true, None)
    }
}

impl HttpRunner {
    pub fn new(validate_certs: bool, proxy: Option<&str>) -> Self {
        Self::new_with_timeout(validate_certs, proxy, Some(30_000))
    }

    pub fn new_with_timeout(
        validate_certs: bool,
        proxy: Option<&str>,
        default_timeout_ms: Option<u64>,
    ) -> Self {
        let cookie_jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
        let user_agent = concat!("paynal/", env!("CARGO_PKG_VERSION"));

        let mut builder = reqwest::Client::builder()
            .user_agent(user_agent)
            .cookie_provider(std::sync::Arc::clone(&cookie_jar))
            .redirect(reqwest::redirect::Policy::limited(10));

        let mut no_redir_builder = reqwest::Client::builder()
            .user_agent(user_agent)
            .cookie_provider(cookie_jar)
            .redirect(reqwest::redirect::Policy::none());

        if let Some(ms) = default_timeout_ms {
            builder = builder.timeout(std::time::Duration::from_millis(ms));
            no_redir_builder = no_redir_builder.timeout(std::time::Duration::from_millis(ms));
        }

        if !validate_certs {
            builder = builder.danger_accept_invalid_certs(true);
            no_redir_builder = no_redir_builder.danger_accept_invalid_certs(true);
        }

        if let Some(proxy_url) = proxy {
            if !proxy_url.trim().is_empty() {
                if let Ok(p) = reqwest::Proxy::all(proxy_url) {
                    builder = builder.proxy(p.clone());
                    no_redir_builder = no_redir_builder.proxy(p);
                }
            }
        }

        Self {
            client: builder.build().unwrap_or_default(),
            no_redirect_client: no_redir_builder.build().unwrap_or_default(),
            default_timeout_ms,
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

        let client = if request.follow_redirects == Some(false) {
            &self.no_redirect_client
        } else {
            &self.client
        };

        let mut req_builder = client.request(method.clone(), &url);

        // Query parameters
        if let Some(params) = &request.params {
            let mut interp_params = Vec::new();
            for (k, v) in params {
                interp_params.push((ctx.interpolate(k), ctx.interpolate(v)));
            }
            req_builder = req_builder.query(&interp_params);
        }

        // P1-1: Request-level timeout override
        if let Some(ms) = request.timeout_ms {
            req_builder = req_builder.timeout(std::time::Duration::from_millis(ms));
        }

        // Add headers
        for (k, v) in &request.headers {
            let key = ctx.interpolate(k);
            let val = ctx.interpolate(v);
            if let Ok(h_name) = HeaderName::from_str(&key) {
                req_builder = req_builder.header(h_name, val);
            }
        }

        // Add auth specification
        if let Some(auth) = &request.auth {
            match auth {
                crate::models::AuthSpec::Bearer { token } => {
                    let interp_token = ctx.interpolate(token);
                    req_builder = req_builder.bearer_auth(interp_token);
                }
                crate::models::AuthSpec::Basic { username, password } => {
                    let interp_user = ctx.interpolate(username);
                    let interp_pass = password.as_ref().map(|p| ctx.interpolate(p));
                    req_builder = req_builder.basic_auth(interp_user, interp_pass);
                }
                crate::models::AuthSpec::ApiKey { key, value, r#in } => {
                    let interp_key = ctx.interpolate(key);
                    let interp_val = ctx.interpolate(value);
                    if r#in.to_lowercase() == "query" {
                        req_builder = req_builder.query(&[(&interp_key, &interp_val)]);
                    } else if let Ok(h_name) = HeaderName::from_str(&interp_key) {
                        req_builder = req_builder.header(h_name, interp_val);
                    }
                }
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

            // 1b. Status in allowed list
            if let Some(allowed) = &asserts.status_in {
                if allowed.contains(&status) {
                    assert_messages.push(format!("Status {} is in allowed list {:?}", status, allowed));
                } else {
                    asserts_passed = false;
                    assert_messages.push(format!("Status mismatch: got {}, expected one of {:?}", status, allowed));
                }
            }

            // 1c. Status range pattern (2xx, 3xx, 4xx, 5xx, or min-max)
            if let Some(range_str) = &asserts.status_range {
                let range_clean = range_str.trim().to_ascii_lowercase();
                let is_match = match range_clean.as_str() {
                    "1xx" => (100..200).contains(&status),
                    "2xx" => (200..300).contains(&status),
                    "3xx" => (300..400).contains(&status),
                    "4xx" => (400..500).contains(&status),
                    "5xx" => (500..600).contains(&status),
                    other => {
                        if let Some((start_s, end_s)) = other.split_once('-') {
                            if let (Ok(start), Ok(end)) = (start_s.trim().parse::<u16>(), end_s.trim().parse::<u16>()) {
                                (start..=end).contains(&status)
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                };

                if is_match {
                    assert_messages.push(format!("Status {} matches range '{}'", status, range_str));
                } else {
                    asserts_passed = false;
                    assert_messages.push(format!("Status {} does NOT match range '{}'", status, range_str));
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
                            let display_expected = Self::mask_sensitive_value(&eval_key, &eval_val);
                            let display_actual = Self::mask_sensitive_value(&eval_key, actual_val);
                            if actual_val == eval_val {
                                assert_messages.push(format!("Header '{}' == '{}'", eval_key, display_expected));
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("Header '{}' mismatch: got '{}', expected '{}'", eval_key, display_actual, display_expected));
                            }
                        }
                        None => {
                            asserts_passed = false;
                            assert_messages.push(format!("Header '{}' not found in response", eval_key));
                        }
                    }
                }
            }

            // 3b. Header Contains assertion
            if let Some(hc) = &asserts.header_contains {
                for (k, expected_sub) in hc {
                    let eval_key = ctx.interpolate(k);
                    let eval_sub = ctx.interpolate(expected_sub);

                    let found_header = headers.iter().find(|(h_name, _)| h_name.as_str().eq_ignore_ascii_case(&eval_key));
                    match found_header {
                        Some((_, val)) => {
                            let actual_val = val.to_str().unwrap_or("");
                            let display_sub = Self::mask_sensitive_value(&eval_key, &eval_sub);
                            let display_actual = Self::mask_sensitive_value(&eval_key, actual_val);
                            if actual_val.contains(&eval_sub) {
                                assert_messages.push(format!("Header '{}' contains '{}'", eval_key, display_sub));
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("Header '{}' value '{}' does not contain '{}'", eval_key, display_actual, display_sub));
                            }
                        }
                        None => {
                            asserts_passed = false;
                            assert_messages.push(format!("Header '{}' not found in response", eval_key));
                        }
                    }
                }
            }

            // 3c. Header Regex assertion
            if let Some(hr) = &asserts.header_regex {
                for (k, pattern) in hr {
                    let eval_key = ctx.interpolate(k);
                    let eval_pat = ctx.interpolate(pattern);

                    let found_header = headers.iter().find(|(h_name, _)| h_name.as_str().eq_ignore_ascii_case(&eval_key));
                    match found_header {
                        Some((_, val)) => {
                            let actual_val = val.to_str().unwrap_or("");
                            let display_actual = Self::mask_sensitive_value(&eval_key, actual_val);
                            match regex::Regex::new(&eval_pat) {
                                Ok(re) => {
                                    if re.is_match(actual_val) {
                                        assert_messages.push(format!("Header '{}' matches regex '{}'", eval_key, eval_pat));
                                    } else {
                                        asserts_passed = false;
                                        assert_messages.push(format!("Header '{}' value '{}' does not match regex '{}'", eval_key, display_actual, eval_pat));
                                    }
                                }
                                Err(e) => {
                                    asserts_passed = false;
                                    assert_messages.push(format!("Invalid regex pattern '{}' for header '{}': {}", eval_pat, eval_key, e));
                                }
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

            // 10. Numeric comparison: GT (>)
            if let Some(gt_map) = &asserts.json_gt {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (expr, limit) in gt_map {
                        let norm = if !expr.starts_with('$') { format!("$.{}", expr) } else { expr.clone() };
                        if let Ok(results) = jsonpath_lib::select(&json_val, &norm) {
                            if let Some(first) = results.first() {
                                if let Some(num) = first.as_f64() {
                                    if num > *limit {
                                        assert_messages.push(format!("JSONPath {} ({}) > {}", norm, num, limit));
                                    } else {
                                        asserts_passed = false;
                                        assert_messages.push(format!("JSONPath {} ({}) is NOT > {}", norm, num, limit));
                                    }
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} is not a number", norm));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found for gt assertion", norm));
                            }
                        }
                    }
                }
            }

            // 11. Numeric comparison: GTE (>=)
            if let Some(gte_map) = &asserts.json_gte {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (expr, limit) in gte_map {
                        let norm = if !expr.starts_with('$') { format!("$.{}", expr) } else { expr.clone() };
                        if let Ok(results) = jsonpath_lib::select(&json_val, &norm) {
                            if let Some(first) = results.first() {
                                if let Some(num) = first.as_f64() {
                                    if num >= *limit {
                                        assert_messages.push(format!("JSONPath {} ({}) >= {}", norm, num, limit));
                                    } else {
                                        asserts_passed = false;
                                        assert_messages.push(format!("JSONPath {} ({}) is NOT >= {}", norm, num, limit));
                                    }
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} is not a number", norm));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found for gte assertion", norm));
                            }
                        }
                    }
                }
            }

            // 12. Numeric comparison: LT (<)
            if let Some(lt_map) = &asserts.json_lt {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (expr, limit) in lt_map {
                        let norm = if !expr.starts_with('$') { format!("$.{}", expr) } else { expr.clone() };
                        if let Ok(results) = jsonpath_lib::select(&json_val, &norm) {
                            if let Some(first) = results.first() {
                                if let Some(num) = first.as_f64() {
                                    if num < *limit {
                                        assert_messages.push(format!("JSONPath {} ({}) < {}", norm, num, limit));
                                    } else {
                                        asserts_passed = false;
                                        assert_messages.push(format!("JSONPath {} ({}) is NOT < {}", norm, num, limit));
                                    }
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} is not a number", norm));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found for lt assertion", norm));
                            }
                        }
                    }
                }
            }

            // 13. Numeric comparison: LTE (<=)
            if let Some(lte_map) = &asserts.json_lte {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (expr, limit) in lte_map {
                        let norm = if !expr.starts_with('$') { format!("$.{}", expr) } else { expr.clone() };
                        if let Ok(results) = jsonpath_lib::select(&json_val, &norm) {
                            if let Some(first) = results.first() {
                                if let Some(num) = first.as_f64() {
                                    if num <= *limit {
                                        assert_messages.push(format!("JSONPath {} ({}) <= {}", norm, num, limit));
                                    } else {
                                        asserts_passed = false;
                                        assert_messages.push(format!("JSONPath {} ({}) is NOT <= {}", norm, num, limit));
                                    }
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} is not a number", norm));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found for lte assertion", norm));
                            }
                        }
                    }
                }
            }

            // 14. Length assertion (array, string, object)
            if let Some(len_map) = &asserts.json_length {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (expr, expected_len) in len_map {
                        let norm = if !expr.starts_with('$') { format!("$.{}", expr) } else { expr.clone() };
                        if let Ok(results) = jsonpath_lib::select(&json_val, &norm) {
                            if let Some(first) = results.first() {
                                let actual_len = match first {
                                    serde_json::Value::Array(a) => Some(a.len()),
                                    serde_json::Value::String(s) => Some(s.len()),
                                    serde_json::Value::Object(o) => Some(o.len()),
                                    _ => None,
                                };
                                if let Some(len) = actual_len {
                                    if len == *expected_len {
                                        assert_messages.push(format!("JSONPath {} length == {}", norm, expected_len));
                                    } else {
                                        asserts_passed = false;
                                        assert_messages.push(format!("JSONPath {} length mismatch: got {}, expected {}", norm, len, expected_len));
                                    }
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} does not support length", norm));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found for length assertion", norm));
                            }
                        }
                    }
                }
            }

            // 15. Type assertion (array, object, string, number, boolean, null)
            if let Some(type_map) = &asserts.json_type {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                    for (expr, expected_type) in type_map {
                        let norm = if !expr.starts_with('$') { format!("$.{}", expr) } else { expr.clone() };
                        if let Ok(results) = jsonpath_lib::select(&json_val, &norm) {
                            if let Some(first) = results.first() {
                                let actual_type = match first {
                                    serde_json::Value::Array(_) => "array",
                                    serde_json::Value::Object(_) => "object",
                                    serde_json::Value::String(_) => "string",
                                    serde_json::Value::Number(_) => "number",
                                    serde_json::Value::Bool(_) => "boolean",
                                    serde_json::Value::Null => "null",
                                };
                                if actual_type.eq_ignore_ascii_case(expected_type) {
                                    assert_messages.push(format!("JSONPath {} type == '{}'", norm, expected_type));
                                } else {
                                    asserts_passed = false;
                                    assert_messages.push(format!("JSONPath {} type mismatch: got '{}', expected '{}'", norm, actual_type, expected_type));
                                }
                            } else {
                                asserts_passed = false;
                                assert_messages.push(format!("JSONPath {} not found for type assertion", norm));
                            }
                        }
                    }
                }
            }

            // 16. JSON Schema validation
            if let Some(schema_spec) = &asserts.schema {
                let schema_content = if schema_spec.trim().starts_with('{') {
                    schema_spec.clone()
                } else {
                    let schema_path = Path::new(schema_spec);
                    match fs::read_to_string(schema_path) {
                        Ok(c) => c,
                        Err(e) => {
                            asserts_passed = false;
                            assert_messages.push(format!("Failed to read JSON schema file '{}': {}", schema_spec, e));
                            String::new()
                        }
                    }
                };

                if !schema_content.is_empty() {
                    match serde_json::from_str::<serde_json::Value>(&schema_content) {
                        Ok(schema_json) => {
                            match jsonschema::validator_for(&schema_json) {
                                Ok(validator) => {
                                    match serde_json::from_str::<serde_json::Value>(&body) {
                                        Ok(body_json) => {
                                            let schema_errors: Vec<String> = validator
                                                .iter_errors(&body_json)
                                                .map(|e| format!("Schema error at {}: {}", e.instance_path(), e))
                                                .collect();
                                            if schema_errors.is_empty() {
                                                assert_messages.push("Response body matches JSON Schema".to_string());
                                            } else {
                                                asserts_passed = false;
                                                assert_messages.extend(schema_errors);
                                            }
                                        }
                                        Err(_) => {
                                            asserts_passed = false;
                                            assert_messages.push("Response body is not valid JSON for schema validation".to_string());
                                        }
                                    }
                                }
                                Err(e) => {
                                    asserts_passed = false;
                                    assert_messages.push(format!("Invalid JSON Schema: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            asserts_passed = false;
                            assert_messages.push(format!("Failed to parse JSON Schema: {}", e));
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

    pub fn is_sensitive_header(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        lower == "authorization"
            || lower == "cookie"
            || lower == "set-cookie"
            || lower == "x-api-key"
            || lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
    }

    pub fn mask_sensitive_value(header_name: &str, val: &str) -> String {
        if Self::is_sensitive_header(header_name) {
            if val.len() <= 8 {
                "***".to_string()
            } else {
                format!("{}...***", &val[..4])
            }
        } else {
            val.to_string()
        }
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

        for (var_name, raw_expr) in captures {
            let expr = ctx.interpolate(raw_expr);
            let mut captured = false;
            let expr_trimmed = expr.trim();

            if expr_trimmed.eq_ignore_ascii_case("status") {
                ctx.set(var_name, res.status.to_string());
                captured = true;
            } else if expr_trimmed.eq_ignore_ascii_case("duration") || expr_trimmed.eq_ignore_ascii_case("latency") {
                ctx.set(var_name, res.duration_ms.to_string());
                captured = true;
            } else if expr_trimmed.eq_ignore_ascii_case("body") {
                ctx.set(var_name, res.body.clone());
                captured = true;
            } else if let Some(pattern) = expr_trimmed.strip_prefix("regex:") {
                if let Ok(re) = regex::Regex::new(pattern) {
                    if let Some(caps) = re.captures(&res.body) {
                        if let Some(m) = caps.get(1).or_else(|| caps.get(0)) {
                            ctx.set(var_name, m.as_str());
                            captured = true;
                        }
                    }
                }
            } else if expr.starts_with("header.") || expr.starts_with("headers.") {
                let header_key = expr
                    .trim_start_matches("headers.")
                    .trim_start_matches("header.");
                for (h_name, h_val) in &res.headers {
                    if h_name.as_str().eq_ignore_ascii_case(header_key) {
                        if let Ok(val_str) = h_val.to_str() {
                            ctx.set(var_name, val_str);
                            captured = true;
                        }
                    }
                }
            } else if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&res.body) {
                // P1-3: Normalize JSONPath capture if missing "$."
                let norm_expr = if !expr.starts_with('$') {
                    format!("$.{}", expr)
                } else {
                    expr.clone()
                };

                if let Ok(results) = jsonpath_lib::select(&json_val, &norm_expr) {
                    if let Some(first) = results.first() {
                        let val_str = match first {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        ctx.set(var_name, val_str);
                        captured = true;
                    }
                }
            }

            if !captured {
                eprintln!(
                    "⚠️  Capture '{}' with expression '{}' produced no match in response",
                    var_name, expr
                );
            }
        }
    }

    pub async fn execute_with_retry(
        &self,
        request: &RequestSpec,
        assert_spec: Option<&AssertSpec>,
        ctx: &VariableContext,
        step_retry: Option<&RetrySpec>,
    ) -> Result<ExecutionResult> {
        let retry_config = step_retry.or(request.retry.as_ref());

        let retry = match retry_config {
            Some(r) if r.attempts() > 1 => r,
            _ => return self.execute(request, assert_spec, ctx).await,
        };

        let attempts = retry.attempts();
        let backoff_ms = retry.backoff_ms();
        let mut last_err = None;

        for attempt in 1..=attempts {
            if attempt > 1 {
                let delay = std::time::Duration::from_millis(backoff_ms * (attempt as u64 - 1));
                tokio::time::sleep(delay).await;
            }

            match self.execute(request, assert_spec, ctx).await {
                Ok(res) => {
                    if attempt < attempts && retry.should_retry_status(res.status) {
                        println!(
                            "   ⚠️ Received HTTP status {}. Retrying attempt {}/{} after backoff...",
                            res.status,
                            attempt + 1,
                            attempts
                        );
                        continue;
                    }
                    return Ok(res);
                }
                Err(e) => {
                    if attempt < attempts {
                        println!(
                            "   ⚠️ Request transport error: {}. Retrying attempt {}/{} after backoff...",
                            e,
                            attempt + 1,
                            attempts
                        );
                        last_err = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Request retries exhausted without a response")))
    }

    pub fn print_result(&self, name: &str, method: &str, url: &str, res: &ExecutionResult) {
        self.print_result_full(name, method, url, res, false, None, None);
    }

    pub fn print_result_full(
        &self,
        name: &str,
        method: &str,
        url: &str,
        res: &ExecutionResult,
        verbose: bool,
        req_headers: Option<&HashMap<String, String>>,
        req_body: Option<&str>,
    ) {
        let status_colored = if res.status < 300 {
            format!("{}", res.status).green().bold()
        } else if res.status < 400 {
            format!("{}", res.status).yellow().bold()
        } else {
            format!("{}", res.status).red().bold()
        };

        println!("\n{}", "─".repeat(60).dimmed());
        println!("🚀 Request: {} [{} {}]", name.bold(), method.cyan(), url);

        if verbose {
            println!("📤 Sent Request (Verbose):");
            if let Some(headers) = req_headers {
                println!("   Headers:");
                for (k, v) in headers {
                    let masked_val = Self::mask_sensitive_value(k, v);
                    println!("     {}: {}", k.cyan(), masked_val);
                }
            }
            if let Some(body) = req_body {
                if !body.is_empty() {
                    println!("   Body:\n{}", body);
                }
            }
        }

        println!(
            "⏱️  Duration: {} ms | Status: {} ({}) | Headers: {}",
            res.duration_ms,
            status_colored,
            res.status_text,
            res.headers.len()
        );

        if verbose && !res.headers.is_empty() {
            println!("📥 Response Headers (Verbose):");
            for (key, val) in &res.headers {
                let val_str = val.to_str().unwrap_or("<binary>");
                let masked = Self::mask_sensitive_value(key.as_str(), val_str);
                println!("   {}: {}", key.as_str().cyan(), masked);
            }
        }
        
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

    pub fn dry_run_preview(
        name: &str,
        method: &str,
        url: &str,
        headers: &HashMap<String, String>,
        body: Option<&str>,
    ) {
        println!("\n{}", "─".repeat(60).yellow());
        println!("🔍 [DRY-RUN] Request: {} [{} {}]", name.bold(), method.cyan(), url);
        if !headers.is_empty() {
            println!("   Headers:");
            for (k, v) in headers {
                let masked = Self::mask_sensitive_value(k, v);
                println!("     {}: {}", k.cyan(), masked);
            }
        }
        if let Some(b) = body {
            if !b.is_empty() {
                println!("   Body:\n{}", b);
            }
        }
        println!("{}\n", "─".repeat(60).yellow());
    }
}
