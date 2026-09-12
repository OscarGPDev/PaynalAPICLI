use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    vars: HashMap<String, String>,
    pub strict_vars: bool,
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

        // 2. Load base .env then paynal.env files (paynal.env overrides .env)
        Self::read_env_file(Path::new(".env"), &mut vars);
        Self::read_env_file(Path::new("paynal.env"), &mut vars);

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

        Self {
            vars,
            strict_vars: false,
        }
    }

    fn read_env_file(path: &Path, vars: &mut HashMap<String, String>) {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                for (k, v) in Self::parse_env_str(&content) {
                    vars.insert(k, v);
                }
            }
        }
    }

    /// Pure, robust parser for `.env` and `paynal.env` files without external dependencies.
    /// Does NOT perform destructive bash-like variable expansions (e.g. `$VAR` remains literal `$VAR`).
    pub fn parse_env_str(content: &str) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        let cleaned = content.trim_start_matches('\u{feff}'); // Strip UTF-8 BOM if present
        let mut lines = cleaned.lines().peekable();

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Strip optional `export ` prefix (e.g., `export KEY=val`)
            let line_to_parse = if let Some(rest) = trimmed.strip_prefix("export") {
                if rest.starts_with(|c: char| c.is_whitespace()) {
                    rest.trim_start()
                } else {
                    trimmed
                }
            } else {
                trimmed
            };

            let (key_part, val_part) = match line_to_parse.split_once('=') {
                Some((k, v)) => (k.trim(), v.trim_start()),
                None => continue,
            };

            if key_part.is_empty() {
                continue;
            }

            let key = key_part.to_string();

            if val_part.starts_with('\'') {
                // Single-quoted: verbatim literal content until closing quote (multiline supported)
                let mut val = String::new();
                let mut current_slice = &val_part[1..];

                loop {
                    if let Some(end_idx) = current_slice.find('\'') {
                        val.push_str(&current_slice[..end_idx]);
                        break;
                    } else {
                        val.push_str(current_slice);
                        val.push('\n');
                        if let Some(next_line) = lines.next() {
                            current_slice = next_line;
                        } else {
                            break;
                        }
                    }
                }
                entries.push((key, val));
            } else if val_part.starts_with('"') {
                // Double-quoted: unescapes standard sequences, preserves literal `$` (multiline supported)
                let mut raw_val = String::new();
                let mut current_slice = &val_part[1..];

                loop {
                    let mut found_end = None;
                    let mut is_escaped = false;
                    for (idx, ch) in current_slice.char_indices() {
                        if is_escaped {
                            is_escaped = false;
                        } else if ch == '\\' {
                            is_escaped = true;
                        } else if ch == '"' {
                            found_end = Some(idx);
                            break;
                        }
                    }

                    if let Some(end_idx) = found_end {
                        raw_val.push_str(&current_slice[..end_idx]);
                        break;
                    } else {
                        raw_val.push_str(current_slice);
                        raw_val.push('\n');
                        if let Some(next_line) = lines.next() {
                            current_slice = next_line;
                        } else {
                            break;
                        }
                    }
                }

                let val = Self::unescape_double_quoted(&raw_val);
                entries.push((key, val));
            } else {
                // Unquoted value:
                // An inline comment `#` is only recognized if preceded by whitespace (e.g. ` # comment`).
                // An inline `#` directly touching characters (e.g. `foo#bar` or `Ab1$xy{z}#w!%2Q`) is part of the value.
                let mut val = String::new();
                let mut chars = val_part.chars().peekable();
                let mut prev_is_whitespace = false;

                while let Some(ch) = chars.next() {
                    if ch == '#' && prev_is_whitespace {
                        break;
                    }
                    if ch == '\\' {
                        if let Some(next_ch) = chars.next() {
                            val.push(next_ch);
                            prev_is_whitespace = next_ch.is_whitespace();
                            continue;
                        }
                    }
                    prev_is_whitespace = ch.is_whitespace();
                    val.push(ch);
                }

                entries.push((key, val.trim_end().to_string()));
            }
        }

        entries
    }

    fn unescape_double_quoted(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(escaped) = chars.next() {
                    match escaped {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        '$' => out.push('$'),
                        other => {
                            out.push('\\');
                            out.push(other);
                        }
                    }
                } else {
                    out.push('\\');
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    pub fn set(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.vars.insert(key.into(), val.into());
    }

    #[allow(dead_code)]
    pub fn get(&self, key: &str) -> Option<&String> {
        self.vars.get(key)
    }

    pub fn extend(&mut self, other: &HashMap<String, String>) {
        // P1-7: Multi-pass fixed-point resolution so that variables inside the same block
        // referencing each other resolve deterministically regardless of HashMap iteration order.
        let mut unresolved: HashMap<String, String> = other.clone();
        let max_passes = other.len().max(1);

        for _ in 0..max_passes {
            let mut resolved_any = false;
            for (k, v) in unresolved.clone() {
                let interpolated = self.interpolate(&v);
                if interpolated != v || !interpolated.contains("${") {
                    self.vars.insert(k.clone(), interpolated.clone());
                    unresolved.insert(k, interpolated);
                    resolved_any = true;
                }
            }
            if !resolved_any {
                break;
            }
        }

        // Final pass to commit all
        for (k, v) in &unresolved {
            let final_val = self.interpolate(v);
            self.vars.insert(k.clone(), final_val);
        }
    }

    /// Validates that there are no unresolved `${VAR}` placeholders in the input string.
    pub fn validate_no_unresolved_placeholders(&self, input: &str) -> anyhow::Result<()> {
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i] == b'\\' && i + 2 < len && bytes[i + 1] == b'$' && bytes[i + 2] == b'{' {
                i += 3;
                continue;
            }

            if bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
                let start_key = i + 2;
                if let Some(close_rel) = input[start_key..].find('}') {
                    let end_key = start_key + close_rel;
                    let key = &input[start_key..end_key];

                    if !self.vars.contains_key(key) && !Self::is_dynamic_variable(key) {
                        anyhow::bail!("Unresolved variable placeholder: ${{{}}}", key);
                    }
                    i = end_key + 1;
                    continue;
                }
            }

            let ch = input[i..].chars().next().unwrap();
            i += ch.len_utf8();
        }

        Ok(())
    }

    pub fn is_dynamic_variable(key: &str) -> bool {
        key == "$uuid"
            || key == "$timestamp"
            || key == "$timestampMs"
            || key == "$isoTimestamp"
            || key == "$isoDate"
            || key == "$randomInt"
            || (key.starts_with("$randomInt(") && key.ends_with(')'))
    }

    pub fn eval_dynamic_variable(key: &str) -> Option<String> {
        if key == "$uuid" {
            return Some(uuid::Uuid::new_v4().to_string());
        }
        if key == "$timestamp" {
            return Some(chrono::Utc::now().timestamp().to_string());
        }
        if key == "$timestampMs" {
            return Some(chrono::Utc::now().timestamp_millis().to_string());
        }
        if key == "$isoTimestamp" || key == "$isoDate" {
            return Some(chrono::Utc::now().to_rfc3339());
        }
        if key == "$randomInt" {
            let rand_bytes = uuid::Uuid::new_v4();
            let n = u64::from_le_bytes(rand_bytes.as_bytes()[..8].try_into().unwrap());
            let val = (n % 1000) + 1;
            return Some(val.to_string());
        }
        if key.starts_with("$randomInt(") && key.ends_with(')') {
            let inside = &key[11..key.len() - 1];
            if let Some((min_s, max_s)) = inside.split_once(',') {
                if let (Ok(min), Ok(max)) = (min_s.trim().parse::<i64>(), max_s.trim().parse::<i64>()) {
                    if min <= max {
                        let range = (max - min + 1) as u64;
                        let rand_bytes = uuid::Uuid::new_v4();
                        let n = u64::from_le_bytes(rand_bytes.as_bytes()[..8].try_into().unwrap());
                        let val = min + (n % range) as i64;
                        return Some(val.to_string());
                    }
                }
            }
        }
        None
    }

    /// Single-pass placeholder interpolation.
    /// Replaces occurrences of `${VAR_NAME}` with corresponding values from the context.
    /// - Escaped `\${VAR}` is replaced with literal `${VAR}` without interpolation.
    /// - Values with `$` (like `Ab1$xy{z}#w!%2Q` or `$100`) are inserted verbatim without re-evaluation.
    /// - Unresolved variables remain as `${VAR_NAME}` and emit a diagnostic warning.
    pub fn interpolate(&self, input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Check for escaped placeholder: \${KEY} -> ${KEY}
            if bytes[i] == b'\\' && i + 2 < len && bytes[i + 1] == b'$' && bytes[i + 2] == b'{' {
                result.push('$');
                result.push('{');
                i += 3;
                continue;
            }

            // Check for placeholder start: ${KEY}
            if bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
                let start_key = i + 2;
                if let Some(close_rel) = input[start_key..].find('}') {
                    let end_key = start_key + close_rel;
                    let key = &input[start_key..end_key];

                    if let Some(val) = self.vars.get(key) {
                        result.push_str(val);
                    } else if let Some(dyn_val) = Self::eval_dynamic_variable(key) {
                        result.push_str(&dyn_val);
                    } else {
                        // P1-4: Emit diagnostic warning on unresolved variable
                        eprintln!("⚠️  Unresolved variable placeholder: ${{{}}}", key);
                        result.push_str(&input[i..=end_key]);
                    }
                    i = end_key + 1;
                    continue;
                }
            }

            // Normal character (UTF-8 safe)
            let ch = input[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_literal_dollar_and_hashes() {
        let env_content = r#"
# User reported test case
SECRET_UNQUOTED=Ab1$xy{z}#w!%2Q
SECRET_DOUBLE="Ab1$xy{z}#w!%2Q"
SECRET_SINGLE='Ab1$xy{z}#w!%2Q'
COMMENTED=foo # this is a comment
NO_SPACE_HASH=foo#bar
PASSWORD=p@$$w0rd!
BCRYPT_HASH=$2a$12$e8NdwKh7Yg
"#;
        let parsed: HashMap<String, String> = VariableContext::parse_env_str(env_content)
            .into_iter()
            .collect();

        assert_eq!(parsed.get("SECRET_UNQUOTED").unwrap(), "Ab1$xy{z}#w!%2Q");
        assert_eq!(parsed.get("SECRET_DOUBLE").unwrap(), "Ab1$xy{z}#w!%2Q");
        assert_eq!(parsed.get("SECRET_SINGLE").unwrap(), "Ab1$xy{z}#w!%2Q");
        assert_eq!(parsed.get("COMMENTED").unwrap(), "foo");
        assert_eq!(parsed.get("NO_SPACE_HASH").unwrap(), "foo#bar");
        assert_eq!(parsed.get("PASSWORD").unwrap(), "p@$$w0rd!");
        assert_eq!(parsed.get("BCRYPT_HASH").unwrap(), "$2a$12$e8NdwKh7Yg");
    }

    #[test]
    fn test_parse_env_multiline_and_escapes() {
        let env_content = r#"
export MULTI_DOUBLE="first line
second line"
export MULTI_SINGLE='single line 1
single line 2'
ESCAPED="hello \"world\"\nnew line\ttab"
"#;
        let parsed: HashMap<String, String> = VariableContext::parse_env_str(env_content)
            .into_iter()
            .collect();

        assert_eq!(
            parsed.get("MULTI_DOUBLE").unwrap(),
            "first line\nsecond line"
        );
        assert_eq!(
            parsed.get("MULTI_SINGLE").unwrap(),
            "single line 1\nsingle line 2"
        );
        assert_eq!(
            parsed.get("ESCAPED").unwrap(),
            "hello \"world\"\nnew line\ttab"
        );
    }

    #[test]
    fn test_single_pass_interpolation_with_dollar() {
        let mut ctx = VariableContext::new();
        ctx.set("TOKEN", "Ab1$xy{z}#w!%2Q");
        ctx.set("xy", "SHOULD_NOT_BE_USED");
        ctx.set("z", "SHOULD_NOT_BE_USED");
        ctx.set("USER_ID", "42");

        // Basic interpolation
        let res = ctx.interpolate("Bearer ${TOKEN}");
        assert_eq!(res, "Bearer Ab1$xy{z}#w!%2Q");

        // Multiple placeholders and unquoted dollars
        let res2 = ctx.interpolate("https://api.com/users/${USER_ID}?token=${TOKEN}&price=$100");
        assert_eq!(
            res2,
            "https://api.com/users/42?token=Ab1$xy{z}#w!%2Q&price=$100"
        );

        // Escaped placeholder
        let res3 = ctx.interpolate(r"Literal \${TOKEN} vs ${USER_ID}");
        assert_eq!(res3, "Literal ${TOKEN} vs 42");

        // Unresolved placeholder preserved
        let res4 = ctx.interpolate("Hello ${UNKNOWN_VAR}");
        assert_eq!(res4, "Hello ${UNKNOWN_VAR}");
    }

    #[test]
    fn test_read_env_file_with_dollar() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let test_env_path = temp_dir.join("test_paynal.env");
        {
            let mut f = std::fs::File::create(&test_env_path).unwrap();
            writeln!(f, "TEST_SECRET=Ab1$xy{{z}}#w!%2Q").unwrap();
            writeln!(f, "OTHER_KEY=\"val with $special\"").unwrap();
        }

        let mut vars = HashMap::new();
        VariableContext::read_env_file(&test_env_path, &mut vars);
        let _ = std::fs::remove_file(test_env_path);

        assert_eq!(vars.get("TEST_SECRET").unwrap(), "Ab1$xy{z}#w!%2Q");
        assert_eq!(vars.get("OTHER_KEY").unwrap(), "val with $special");

        let mut ctx = VariableContext::new();
        ctx.extend(&vars);
        assert_eq!(
            ctx.interpolate("Auth: ${TEST_SECRET}"),
            "Auth: Ab1$xy{z}#w!%2Q"
        );
    }

    #[test]
    fn test_dynamic_variables_evaluation() {
        let ctx = VariableContext::new();

        // 1. UUID
        let uuid_val = ctx.interpolate("${$uuid}");
        assert_eq!(uuid_val.len(), 36);
        assert_eq!(uuid_val.chars().filter(|c| *c == '-').count(), 4);

        // 2. Timestamp
        let ts_val = ctx.interpolate("${$timestamp}");
        assert!(ts_val.parse::<i64>().is_ok());

        // 3. RandomInt range
        for _ in 0..20 {
            let rnd_val = ctx.interpolate("${$randomInt(50, 60)}");
            let n: i64 = rnd_val.parse().expect("should parse integer");
            assert!(n >= 50 && n <= 60, "Random integer {} out of range [50, 60]", n);
        }

        // 4. Validate placeholders with dynamic variables passes
        assert!(ctx.validate_no_unresolved_placeholders("user_${$uuid}_${$timestamp}").is_ok());
    }
}
