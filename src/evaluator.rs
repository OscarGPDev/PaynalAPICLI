use std::collections::HashMap;
use dotenvy;
use std::env;

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    vars: HashMap<String, String>,
}

impl VariableContext {
    pub fn new() -> Self {
        let mut vars = HashMap::new();

        // 1. Load system environment & paynal.env
        let _ = dotenvy::from_filename("paynal.env");
        let _ = dotenvy::dotenv(); // .env fallback

        for (k, v) in env::vars() {
            vars.insert(k, v);
        }

        Self { vars }
    }

    pub fn set(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.vars.insert(key.into(), val.into());
    }

    pub fn extend(&mut self, other: &HashMap<String, String>) {
        for (k, v) in other {
            // Interpolate existing variables inside newly provided variables
            let evaluated_val = self.interpolate(v);
            self.vars.insert(k.clone(), evaluated_val);
        }
    }

    pub fn interpolate(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (key, val) in &self.vars {
            let placeholder = format!("${{{}}}", key);
            result = result.replace(&placeholder, val);
        }
        result
    }
}
