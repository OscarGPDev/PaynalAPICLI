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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestSpec {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default, alias = "formData", alias = "form_data")]
    pub form_data: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
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

    #[serde(default)]
    pub regex: Option<MatchSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoutineStep {
    pub id: String,
    pub name: Option<String>,
    pub request: RequestSpec,
    #[serde(default)]
    pub capture: HashMap<String, String>,
    #[serde(default)]
    pub assert: Option<AssertSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaynalFile {
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub vars: HashMap<String, String>,
    
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
