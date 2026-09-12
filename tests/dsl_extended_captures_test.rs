use paynal::evaluator::VariableContext;
use paynal::models::RequestSpec;
use paynal::runner::HttpRunner;
use std::collections::HashMap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_test_runner() -> HttpRunner {
    HttpRunner::new(true, None)
}

#[tokio::test]
async fn test_capture_status_duration_body_and_regex() {
    let mock_server = MockServer::start().await;

    let html_body = r#"
    <!DOCTYPE html>
    <html>
      <head><title>Test Page</title></head>
      <body>
        <input type="hidden" name="_csrf" value="csrf-token-secret-98765" />
        <span id="session-code">SESS_44321</span>
      </body>
    </html>
    "#;

    Mock::given(method("GET"))
        .and(path("/form"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html_body))
        .mount(&mock_server)
        .await;

    let runner = create_test_runner();
    let mut ctx = VariableContext::new();

    let req = RequestSpec {
        method: "GET".to_string(),
        url: format!("{}/form", mock_server.uri()),
        ..Default::default()
    };

    let res = runner.execute(&req, None, &ctx).await.unwrap();

    let mut captures = HashMap::new();
    captures.insert("httpStatus".to_string(), "status".to_string());
    captures.insert("httpDuration".to_string(), "duration".to_string());
    captures.insert("htmlBody".to_string(), "body".to_string());
    captures.insert("csrfToken".to_string(), r#"regex:name="_csrf" value="([^"]+)""#.to_string());
    captures.insert("sessionCode".to_string(), r#"regex:id="session-code">([^<]+)<"#.to_string());

    runner.extract_captures(&res, &captures, &mut ctx);

    assert_eq!(ctx.get("httpStatus").map(|s| s.as_str()), Some("200"));
    assert!(ctx.get("httpDuration").is_some());
    assert!(ctx.get("htmlBody").unwrap().contains("Test Page"));
    assert_eq!(ctx.get("csrfToken").map(|s| s.as_str()), Some("csrf-token-secret-98765"));
    assert_eq!(ctx.get("sessionCode").map(|s| s.as_str()), Some("SESS_44321"));
}
