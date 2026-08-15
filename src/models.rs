use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestSpec {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssertSpec {
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub json: Option<HashMap<String, serde_json::Value>>,
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
