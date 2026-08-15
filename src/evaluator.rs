use std::collections::HashMap;
use std::env;
use std::path::Path;
use dotenvy;

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    vars: HashMap<String, String>,
}

impl VariableContext {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::new_with_env(None)
    }

    pub fn new_with_env(env_profile: Option<&str>) -> Self {
        let mut vars = HashMap::new();

        // 1. Load system environment
        for (k, v) in env::vars() {
            vars.insert(k, v);
        }

        // 2. Load base paynal.env and .env files
        Self::read_env_file(Path::new("paynal.env"), &mut vars);
        Self::read_env_file(Path::new(".env"), &mut vars);

        // 3. Load profile-specific env file (e.g. paynal.env.staging) with override priority
        if let Some(profile) = env_profile {
            let profile_filename = format!("paynal.env.{}", profile);
            let fallback_filename = format!(".env.{}", profile);

            let profile_path = Path::new(&profile_filename);
            if profile_path.exists() {
                Self::read_env_file(profile_path, &mut vars);
            } else {
                Self::read_env_file(Path::new(&fallback_filename), &mut vars);
            }
        }

        Self { vars }
    }

    fn read_env_file(path: &Path, vars: &mut HashMap<String, String>) {
        if path.exists() {
            if let Ok(iter) = dotenvy::from_path_iter(path) {
                for (k, v) in iter.flatten() {
                    vars.insert(k, v);
                }
            }
        }
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
