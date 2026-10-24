use paynal::manifest::{default_output_sections, PaynalManifest};
use paynal::models::{PaynalFile, ResolvedOutputSections};

#[test]
fn test_manifest_output_sections_default() {
    let manifest = PaynalManifest::default();
    assert_eq!(
        manifest.output_sections,
        vec!["status", "payload", "headers", "asserts", "body"]
    );
    assert_eq!(
        default_output_sections(),
        vec!["status", "payload", "headers", "asserts", "body"]
    );
}

#[test]
fn test_manifest_output_sections_deserialization() {
    let json_data = r#"{
        "projectName": "Test",
        "version": "1.0.0",
        "rootDir": ".",
        "outDir": "./out",
        "docsDir": "./docs",
        "resources": [],
        "maxThreads": "4",
        "defaultExportFormat": "curl",
        "outputSections": ["status", "body"]
    }"#;
    let manifest: PaynalManifest = serde_json::from_str(json_data).unwrap();
    assert_eq!(manifest.output_sections, vec!["status", "body"]);

    let resolved = ResolvedOutputSections::resolve(None, None, None, &manifest.output_sections, false);
    assert!(resolved.show_status);
    assert!(resolved.show_body);
    assert!(!resolved.show_headers);
    assert!(!resolved.show_payload);
    assert!(!resolved.show_asserts);
}

#[test]
fn test_yaml_hierarchy_routine_step_override() {
    let yaml = r#"
version: "1"
name: "Routine Test"
output_sections:
  - status
  - headers
steps:
  - id: "step-1"
    name: "Step with custom sections"
    outputSections:
      - asserts
    request:
      method: "GET"
      url: "https://httpbin.org/get"
  - id: "step-2"
    name: "Step with request-level sections"
    output_sections:
      - asserts
    request:
      method: "POST"
      url: "https://httpbin.org/post"
      output_sections:
        - payload
        - body
"#;
    let file: PaynalFile = serde_yaml::from_str(yaml).unwrap();
    let global = vec!["status".to_string()];

    // Step 1: step-level outputSections overrides file-level
    let step1 = &file.steps[0];
    let resolved_step1 = ResolvedOutputSections::resolve(
        step1.request.output_sections.as_ref(),
        step1.output_sections.as_ref(),
        file.output_sections.as_ref(),
        &global,
        false,
    );
    assert!(resolved_step1.show_asserts);
    assert!(!resolved_step1.show_headers);
    assert!(!resolved_step1.show_status);

    // Step 2: request-level output_sections overrides step-level and file-level
    let step2 = &file.steps[1];
    let resolved_step2 = ResolvedOutputSections::resolve(
        step2.request.output_sections.as_ref(),
        step2.output_sections.as_ref(),
        file.output_sections.as_ref(),
        &global,
        false,
    );
    assert!(resolved_step2.show_payload);
    assert!(resolved_step2.show_body);
    assert!(!resolved_step2.show_asserts);
    assert!(!resolved_step2.show_headers);
}
