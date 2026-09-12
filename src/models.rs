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
}

impl PaynalFile {
    pub fn is_routine(&self) -> bool {
        !self.steps.is_empty()
    }
}
