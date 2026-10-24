use anyhow::Result;
use clap::ValueEnum;
use colored::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReporterType {
    #[default]
    Human,
    Json,
    Junit,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestReport {
    pub total_files: usize,
    pub passed_files: usize,
    pub failed_files: usize,
    pub errored_files: usize,
    pub total_steps: usize,
    pub passed_steps: usize,
    pub failed_steps: usize,
    pub skipped_steps: usize,
    pub total_duration_ms: u128,
    pub files: Vec<FileReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileReport {
    pub name: String,
    pub path: String,
    pub is_routine: bool,
    pub duration_ms: u128,
    pub passed: bool,
    pub error: Option<String>,
    pub steps: Vec<StepReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StepReport {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u128,
    pub passed: bool,
    pub skipped: bool,
    pub asserts: Vec<String>,
    pub failure_messages: Vec<String>,
    pub error: Option<String>,
}

impl TestReport {
    pub fn compute_summary(&mut self) {
        self.total_files = self.files.len();
        self.passed_files = 0;
        self.failed_files = 0;
        self.errored_files = 0;

        self.total_steps = 0;
        self.passed_steps = 0;
        self.failed_steps = 0;
        self.skipped_steps = 0;

        for file in &self.files {
            if file.error.is_some() {
                self.errored_files += 1;
            } else if file.passed {
                self.passed_files += 1;
            } else {
                self.failed_files += 1;
            }

            for step in &file.steps {
                self.total_steps += 1;
                if step.skipped {
                    self.skipped_steps += 1;
                } else if step.passed {
                    self.passed_steps += 1;
                } else {
                    self.failed_steps += 1;
                }
            }
        }
    }

    pub fn to_human_summary(&self) -> String {
        let files_failed_colored = if self.failed_files > 0 {
            self.failed_files.to_string().red().bold()
        } else {
            self.failed_files.to_string().normal()
        };

        let files_errored_colored = if self.errored_files > 0 {
            self.errored_files.to_string().red().bold()
        } else {
            self.errored_files.to_string().normal()
        };

        let steps_failed_colored = if self.failed_steps > 0 {
            self.failed_steps.to_string().red().bold()
        } else {
            self.failed_steps.to_string().normal()
        };

        format!(
            "{}\n📊 Execution Summary (CI Aggregated):\n   Files: {} total | {} passed | {} failed | {} errors\n   Steps: {} total | {} passed | {} failed | {} skipped\n   Total Duration: {} ms\n{}\n",
            "═".repeat(65).cyan(),
            self.total_files,
            self.passed_files.to_string().green().bold(),
            files_failed_colored,
            files_errored_colored,
            self.total_steps,
            self.passed_steps.to_string().green().bold(),
            steps_failed_colored,
            self.skipped_steps.to_string().yellow(),
            self.total_duration_ms,
            "═".repeat(65).cyan()
        )
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_junit_xml(&self) -> String {
        let total_time_sec = (self.total_duration_ms as f64) / 1000.0;
        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str(&format!(
            "<testsuites name=\"Paynal\" tests=\"{}\" failures=\"{}\" errors=\"{}\" time=\"{:.3}\">\n",
            self.total_steps,
            self.failed_steps,
            self.errored_files,
            total_time_sec
        ));

        for file in &self.files {
            let file_time_sec = (file.duration_ms as f64) / 1000.0;
            let file_failures = file.steps.iter().filter(|s| !s.passed && !s.skipped).count();
            let file_errors = if file.error.is_some() { 1 } else { 0 };

            xml.push_str(&format!(
                "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" time=\"{:.3}\">\n",
                xml_escape(&file.name),
                file.steps.len(),
                file_failures,
                file_errors,
                file_time_sec
            ));

            if let Some(err) = &file.error {
                xml.push_str(&format!(
                    "    <testcase classname=\"{}\" name=\"Initialization\" time=\"0.000\">\n",
                    xml_escape(&file.name)
                ));
                xml.push_str(&format!(
                    "      <error message=\"{}\">{}</error>\n",
                    xml_escape(err),
                    xml_escape(err)
                ));
                xml.push_str("    </testcase>\n");
            }

            for step in &file.steps {
                let step_time_sec = (step.duration_ms as f64) / 1000.0;
                xml.push_str(&format!(
                    "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\">\n",
                    xml_escape(&file.name),
                    xml_escape(&step.name),
                    step_time_sec
                ));

                if step.skipped {
                    xml.push_str("      <skipped/>\n");
                } else if !step.passed {
                    let main_msg = step
                        .failure_messages
                        .first()
                        .map(|s| s.as_str())
                        .or(step.error.as_deref())
                        .unwrap_or("Assertion failed");
                    let full_body = step.failure_messages.join("\n");
                    xml.push_str(&format!(
                        "      <failure message=\"{}\">{}</failure>\n",
                        xml_escape(main_msg),
                        xml_escape(&full_body)
                    ));
                }

                xml.push_str("    </testcase>\n");
            }

            xml.push_str("  </testsuite>\n");
        }

        xml.push_str("</testsuites>\n");
        xml
    }
}

pub fn strip_ansi(input: &str) -> String {
    static ANSI_RE: OnceLock<Regex> = OnceLock::new();
    let re = ANSI_RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap());
    re.replace_all(input, "").to_string()
}

pub fn xml_escape(input: &str) -> String {
    let cleaned = strip_ansi(input);
    let mut out = String::with_capacity(cleaned.len());
    for c in cleaned.chars() {
        if ((c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r') || c == '\x7f' {
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn write_report<P: AsRef<Path>>(
    report: &TestReport,
    reporter: ReporterType,
    out_path: Option<P>,
) -> Result<String> {
    let output = match reporter {
        ReporterType::Human => report.to_human_summary(),
        ReporterType::Json => report.to_json()?,
        ReporterType::Junit => report.to_junit_xml(),
    };

    if let Some(path) = out_path {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(p, &output)?;
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape_control_chars() {
        let raw = "Hello \x1b[31mWorld\x00! & < > \" ' \t\r\n";
        let escaped = xml_escape(raw);
        assert_eq!(escaped, "Hello World! &amp; &lt; &gt; &quot; &apos; \t\r\n");
    }
}


