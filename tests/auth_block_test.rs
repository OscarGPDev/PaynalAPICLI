use paynal::models::{AuthSpec, RequestSpec};
use paynal::runner::HttpRunner;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_auth_bearer_and_user_agent() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/bearer-test"))
        .and(header("authorization", "Bearer secret-token-456"))
        .and(header("user-agent", "paynal/0.1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Bearer OK"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let runner = HttpRunner::default();
    let request = RequestSpec {
        url: format!("{}/bearer-test", mock_server.uri()),
        method: "GET".to_string(),
        auth: Some(AuthSpec::Bearer {
            token: "secret-token-456".to_string(),
        }),
        ..Default::default()
    };

    let res = runner.execute(&request, None, &Default::default()).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.body, "Bearer OK");
}

#[tokio::test]
async fn test_auth_basic() {
    let mock_server = MockServer::start().await;

    // user:pass in base64 is dXNlcjpwYXNz
    Mock::given(method("GET"))
        .and(path("/basic-test"))
        .and(header("authorization", "Basic dXNlcjpwYXNz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Basic OK"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let runner = HttpRunner::default();
    let request = RequestSpec {
        url: format!("{}/basic-test", mock_server.uri()),
        method: "GET".to_string(),
        auth: Some(AuthSpec::Basic {
            username: "user".to_string(),
            password: Some("pass".to_string()),
        }),
        ..Default::default()
    };

    let res = runner.execute(&request, None, &Default::default()).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.body, "Basic OK");
}

#[tokio::test]
async fn test_auth_api_key_header_and_query() {
    let mock_server = MockServer::start().await;

    // Header ApiKey
    Mock::given(method("GET"))
        .and(path("/apikey-header"))
        .and(header("x-api-key", "my-secret-key-999"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ApiKey Header OK"))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Query ApiKey
    Mock::given(method("GET"))
        .and(path("/apikey-query"))
        .and(query_param("api_key", "my-query-key-888"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ApiKey Query OK"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let runner = HttpRunner::default();

    let req_header = RequestSpec {
        url: format!("{}/apikey-header", mock_server.uri()),
        method: "GET".to_string(),
        auth: Some(AuthSpec::ApiKey {
            key: "X-API-Key".to_string(),
            value: "my-secret-key-999".to_string(),
            r#in: "header".to_string(),
        }),
        ..Default::default()
    };
    let res1 = runner.execute(&req_header, None, &Default::default()).await.unwrap();
    assert_eq!(res1.status, 200);
    assert_eq!(res1.body, "ApiKey Header OK");

    let req_query = RequestSpec {
        url: format!("{}/apikey-query", mock_server.uri()),
        method: "GET".to_string(),
        auth: Some(AuthSpec::ApiKey {
            key: "api_key".to_string(),
            value: "my-query-key-888".to_string(),
            r#in: "query".to_string(),
        }),
        ..Default::default()
    };
    let res2 = runner.execute(&req_query, None, &Default::default()).await.unwrap();
    assert_eq!(res2.status, 200);
    assert_eq!(res2.body, "ApiKey Query OK");
}
