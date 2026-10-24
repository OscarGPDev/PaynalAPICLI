use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrVec {
    Single(String),
    List(Vec<String>),
}

impl StringOrVec {
    #[allow(dead_code)]
    pub fn into_vec(self) -> Vec<String> {
        match self {
            Self::Single(s) => vec![s],
            Self::List(v) => v,
        }
    }

    pub fn as_slice(&self) -> &[String] {
        match self {
            Self::Single(s) => std::slice::from_ref(s),
            Self::List(v) => v.as_slice(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MatchSpec {
    Single(String),
    List(Vec<String>),
    Map(HashMap<String, String>),
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct RequestSpec {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub params: Option<HashMap<String, String>>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default, alias = "formData", alias = "form_data")]
    pub form_data: Option<HashMap<String, String>>,
    #[serde(default, alias = "timeout", alias = "timeoutMs", alias = "timeout_ms")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub retry: Option<RetrySpec>,
    #[serde(default, alias = "followRedirects", alias = "follow_redirects")]
    pub follow_redirects: Option<bool>,
    #[serde(default)]
    pub auth: Option<AuthSpec>,
    #[serde(default, alias = "outputSections", alias = "output_sections")]
    pub output_sections: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthSpec {
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        #[serde(default)]
        password: Option<String>,
    },
    #[serde(rename = "apikey", alias = "api_key", alias = "api-key")]
    ApiKey {
        key: String,
        value: String,
        #[serde(default = "default_api_key_in")]
        r#in: String,
    },
}

fn default_api_key_in() -> String {
    "header".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum RetrySpec {
    Simple(u32),
    Detailed {
        attempts: u32,
        #[serde(default = "default_retry_backoff_ms", alias = "backoffMs", alias = "backoff_ms")]
        backoff_ms: u64,
        #[serde(default)]
        on: Option<Vec<u16>>,
    },
}

fn default_retry_backoff_ms() -> u64 {
    500
}

impl RetrySpec {
    pub fn attempts(&self) -> u32 {
        match self {
            RetrySpec::Simple(n) => *n,
            RetrySpec::Detailed { attempts, .. } => *attempts,
        }
    }

    pub fn backoff_ms(&self) -> u64 {
        match self {
            RetrySpec::Simple(_) => 500,
            RetrySpec::Detailed { backoff_ms, .. } => *backoff_ms,
        }
    }

    pub fn should_retry_status(&self, status: u16) -> bool {
        match self {
            RetrySpec::Simple(_) => status >= 500,
            RetrySpec::Detailed { on, .. } => {
                if let Some(codes) = on {
                    codes.contains(&status)
                } else {
                    status >= 500
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct AssertSpec {
    #[serde(default)]
    pub status: Option<u16>,

    #[serde(default, alias = "maxDuration", alias = "maxDurationMs", alias = "max_duration_ms", alias = "latencyMs")]
    pub max_duration_ms: Option<u128>,

    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,

    #[serde(default)]
    pub json: Option<HashMap<String, serde_json::Value>>,

    #[serde(default, alias = "present")]
    pub exists: Option<StringOrVec>,

    #[serde(default, alias = "notExists", alias = "not_exists", alias = "notPresent", alias = "not_present", alias = "missing")]
    pub not_exists: Option<StringOrVec>,

    #[serde(default, alias = "bodyContains", alias = "body_contains")]
    pub contains: Option<MatchSpec>,

    #[serde(default, alias = "iContains", alias = "icontains", alias = "containsIgnoreCase", alias = "contains_ignore_case")]
    pub icontains: Option<MatchSpec>,

    #[serde(default, alias = "notContains", alias = "not_contains")]
    pub not_contains: Option<MatchSpec>,

    #[serde(default, alias = "notIcontains", alias = "not_icontains", alias = "notIContains", alias = "notContainsIgnoreCase", alias = "not_contains_ignore_case")]
    pub not_icontains: Option<MatchSpec>,

    #[serde(default, alias = "statusRange", alias = "status_range")]
    pub status_range: Option<String>,

    #[serde(default, alias = "statusIn", alias = "status_in")]
    pub status_in: Option<Vec<u16>>,

    #[serde(default)]
    pub regex: Option<MatchSpec>,

    #[serde(default, alias = "headerContains", alias = "header_contains")]
    pub header_contains: Option<HashMap<String, String>>,

    #[serde(default, alias = "headerRegex", alias = "header_regex")]
    pub header_regex: Option<HashMap<String, String>>,

    #[serde(default, alias = "jsonGt", alias = "gt")]
    pub json_gt: Option<HashMap<String, f64>>,

    #[serde(default, alias = "jsonGte", alias = "gte")]
    pub json_gte: Option<HashMap<String, f64>>,

    #[serde(default, alias = "jsonLt", alias = "lt")]
    pub json_lt: Option<HashMap<String, f64>>,

    #[serde(default, alias = "jsonLte", alias = "lte")]
    pub json_lte: Option<HashMap<String, f64>>,

    #[serde(default, alias = "jsonLength", alias = "length")]
    pub json_length: Option<HashMap<String, usize>>,

    #[serde(default, alias = "jsonType", alias = "type")]
    pub json_type: Option<HashMap<String, String>>,

    #[serde(default)]
    pub schema: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RoutineStep {
    pub id: String,
    pub name: Option<String>,
    pub request: RequestSpec,
    #[serde(default)]
    pub vars: HashMap<String, String>,
    #[serde(default)]
    pub capture: HashMap<String, String>,
    #[serde(default)]
    pub assert: Option<AssertSpec>,
    #[serde(default, alias = "continueOnFailure", alias = "continue_on_failure")]
    pub continue_on_failure: Option<bool>,
    #[serde(default)]
    pub retry: Option<RetrySpec>,
    #[serde(default, alias = "outputSections", alias = "output_sections")]
    pub output_sections: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct PaynalFile {
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub vars: HashMap<String, String>,
    #[serde(default, alias = "continueOnFailure", alias = "continue_on_failure")]
    pub continue_on_failure: Option<bool>,
    
    // Single request payload (if not a multi-step routine)
    pub request: Option<RequestSpec>,
    pub assert: Option<AssertSpec>,

    // Multi-step routine payload
    #[serde(default)]
    pub steps: Vec<RoutineStep>,

    #[serde(default, alias = "outputSections", alias = "output_sections")]
    pub output_sections: Option<Vec<String>>,
}

impl PaynalFile {
    pub fn is_routine(&self) -> bool {
        !self.steps.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedOutputSections {
    pub show_status: bool,
    pub show_payload: bool,
    pub show_headers: bool,
    pub show_asserts: bool,
    pub show_body: bool,
}

impl ResolvedOutputSections {
    pub fn all() -> Self {
        Self {
            show_status: true,
            show_payload: true,
            show_headers: true,
            show_asserts: true,
            show_body: true,
        }
    }

    pub fn from_slice(sections: &[String]) -> Self {
        let mut res = Self {
            show_status: false,
            show_payload: false,
            show_headers: false,
            show_asserts: false,
            show_body: false,
        };

        for s in sections {
            match s.trim().to_ascii_lowercase().as_str() {
                "all" | "*" | "full" => {
                    res.show_status = true;
                    res.show_payload = true;
                    res.show_headers = true;
                    res.show_asserts = true;
                    res.show_body = true;
                }
                "status" | "summary" | "meta" | "duration" | "latency" => {
                    res.show_status = true;
                }
                "payload" | "request" | "req" | "sent" | "request_body" | "request_headers" => {
                    res.show_payload = true;
                }
                "headers" | "response_headers" | "res_headers" => {
                    res.show_headers = true;
                }
                "asserts" | "assertions" | "assert" | "tests" => {
                    res.show_asserts = true;
                }
                "body" | "response_body" | "res_body" | "response" => {
                    res.show_body = true;
                }
                _ => {}
            }
        }

        res
    }

    pub fn resolve(
        req_sections: Option<&Vec<String>>,
        step_sections: Option<&Vec<String>>,
        file_sections: Option<&Vec<String>>,
        global_sections: &[String],
        verbose_flag: bool,
    ) -> Self {
        if verbose_flag {
            return Self::all();
        }

        if let Some(sections) = req_sections {
            Self::from_slice(sections)
        } else if let Some(sections) = step_sections {
            Self::from_slice(sections)
        } else if let Some(sections) = file_sections {
            Self::from_slice(sections)
        } else {
            Self::from_slice(global_sections)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_output_sections_all() {
        let all = ResolvedOutputSections::all();
        assert!(all.show_status);
        assert!(all.show_payload);
        assert!(all.show_headers);
        assert!(all.show_asserts);
        assert!(all.show_body);
    }

    #[test]
    fn test_from_slice_empty() {
        let empty = ResolvedOutputSections::from_slice(&[]);
        assert!(!empty.show_status);
        assert!(!empty.show_payload);
        assert!(!empty.show_headers);
        assert!(!empty.show_asserts);
        assert!(!empty.show_body);
    }

    #[test]
    fn test_from_slice_individual_and_aliases() {
        // Status and aliases
        for alias in &["status", "summary", "meta", "duration", "latency"] {
            let res = ResolvedOutputSections::from_slice(&[alias.to_string()]);
            assert!(res.show_status, "Failed for alias {}", alias);
            assert!(!res.show_payload);
            assert!(!res.show_headers);
            assert!(!res.show_asserts);
            assert!(!res.show_body);
        }

        // Payload and aliases
        for alias in &["payload", "request", "req", "sent", "request_body", "request_headers"] {
            let res = ResolvedOutputSections::from_slice(&[alias.to_string()]);
            assert!(res.show_payload, "Failed for alias {}", alias);
            assert!(!res.show_status);
            assert!(!res.show_headers);
            assert!(!res.show_asserts);
            assert!(!res.show_body);
        }

        // Headers and aliases
        for alias in &["headers", "response_headers", "res_headers"] {
            let res = ResolvedOutputSections::from_slice(&[alias.to_string()]);
            assert!(res.show_headers, "Failed for alias {}", alias);
            assert!(!res.show_status);
            assert!(!res.show_payload);
            assert!(!res.show_asserts);
            assert!(!res.show_body);
        }

        // Asserts and aliases
        for alias in &["asserts", "assertions", "assert", "tests"] {
            let res = ResolvedOutputSections::from_slice(&[alias.to_string()]);
            assert!(res.show_asserts, "Failed for alias {}", alias);
            assert!(!res.show_status);
            assert!(!res.show_payload);
            assert!(!res.show_headers);
            assert!(!res.show_body);
        }

        // Body and aliases
        for alias in &["body", "response_body", "res_body", "response"] {
            let res = ResolvedOutputSections::from_slice(&[alias.to_string()]);
            assert!(res.show_body, "Failed for alias {}", alias);
            assert!(!res.show_status);
            assert!(!res.show_payload);
            assert!(!res.show_headers);
            assert!(!res.show_asserts);
        }

        // All and aliases
        for alias in &["all", "*", "full"] {
            let res = ResolvedOutputSections::from_slice(&[alias.to_string()]);
            assert_eq!(res, ResolvedOutputSections::all(), "Failed for all alias {}", alias);
        }
    }

    #[test]
    fn test_from_slice_combinations() {
        let res = ResolvedOutputSections::from_slice(&["headers".to_string(), "body".to_string()]);
        assert!(!res.show_status);
        assert!(!res.show_payload);
        assert!(res.show_headers);
        assert!(!res.show_asserts);
        assert!(res.show_body);

        let res_single = ResolvedOutputSections::from_slice(&["asserts".to_string()]);
        assert_eq!(
            res_single,
            ResolvedOutputSections {
                show_status: false,
                show_payload: false,
                show_headers: false,
                show_asserts: true,
                show_body: false,
            }
        );

        let res_payload = ResolvedOutputSections::from_slice(&["payload".to_string()]);
        assert_eq!(
            res_payload,
            ResolvedOutputSections {
                show_status: false,
                show_payload: true,
                show_headers: false,
                show_asserts: false,
                show_body: false,
            }
        );
    }

    #[test]
    fn test_resolve_hierarchy() {
        let global = vec!["status".to_string()];
        let file_sec = vec!["headers".to_string()];
        let step_sec = vec!["asserts".to_string()];
        let req_sec = vec!["body".to_string()];

        // 1. verbose_flag overrides all
        let res_verbose = ResolvedOutputSections::resolve(
            Some(&req_sec),
            Some(&step_sec),
            Some(&file_sec),
            &global,
            true,
        );
        assert_eq!(res_verbose, ResolvedOutputSections::all());

        // 2. Request level takes precedence over Step, File, Global
        let res_req = ResolvedOutputSections::resolve(
            Some(&req_sec),
            Some(&step_sec),
            Some(&file_sec),
            &global,
            false,
        );
        assert!(res_req.show_body);
        assert!(!res_req.show_asserts);
        assert!(!res_req.show_headers);
        assert!(!res_req.show_status);

        // 3. Step level takes precedence over File, Global
        let res_step = ResolvedOutputSections::resolve(
            None,
            Some(&step_sec),
            Some(&file_sec),
            &global,
            false,
        );
        assert!(res_step.show_asserts);
        assert!(!res_step.show_body);
        assert!(!res_step.show_headers);
        assert!(!res_step.show_status);

        // 4. File level takes precedence over Global
        let res_file = ResolvedOutputSections::resolve(
            None,
            None,
            Some(&file_sec),
            &global,
            false,
        );
        assert!(res_file.show_headers);
        assert!(!res_file.show_asserts);
        assert!(!res_file.show_status);

        // 5. Fallback to Global
        let res_global = ResolvedOutputSections::resolve(
            None,
            None,
            None,
            &global,
            false,
        );
        assert!(res_global.show_status);
        assert!(!res_global.show_headers);
        assert!(!res_global.show_asserts);
        assert!(!res_global.show_body);
    }

    #[test]
    fn test_yaml_deserialization_output_sections() {
        let yaml_content = r#"
version: "1"
name: "Output Sections Test"
outputSections:
  - status
  - body
request:
  method: "GET"
  url: "https://example.com"
  output_sections:
    - asserts
"#;
        let file: PaynalFile = serde_yaml::from_str(yaml_content).unwrap();
        assert_eq!(file.output_sections, Some(vec!["status".to_string(), "body".to_string()]));
        assert_eq!(file.request.unwrap().output_sections, Some(vec!["asserts".to_string()]));
    }
}
