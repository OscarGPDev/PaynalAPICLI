use paynal::models::RequestSpec;
use paynal::runner::HttpRunner;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_follow_redirects_false() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/redirect"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/destination"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/destination"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Reached"))
        .expect(0)
        .mount(&mock_server)
        .await;

    let runner = HttpRunner::default();
    let request = RequestSpec {
        url: format!("{}/redirect", mock_server.uri()),
        method: "GET".to_string(),
        follow_redirects: Some(false),
        ..Default::default()
    };

    let result = runner.execute(&request, None, &Default::default()).await.unwrap();
    assert_eq!(result.status, 302);
    assert_eq!(
        result.headers.get("location").and_then(|s| s.to_str().ok()),
        Some("/destination")
    );
}

#[tokio::test]
async fn test_follow_redirects_default_true() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/redirect"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/destination"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/destination"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Reached"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let runner = HttpRunner::default();
    let request = RequestSpec {
        url: format!("{}/redirect", mock_server.uri()),
        method: "GET".to_string(),
        follow_redirects: None, // defaults to following redirects
        ..Default::default()
    };

    let result = runner.execute(&request, None, &Default::default()).await.unwrap();
    assert_eq!(result.status, 200);
    assert_eq!(result.body, "Reached");
}

#[tokio::test]
async fn test_cookie_jar_persistence() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Set-Cookie", "session_id=abc123xyz; Path=/; HttpOnly")
                .set_body_string("Logged in"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/profile"))
        .and(header("cookie", "session_id=abc123xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User profile"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let runner = HttpRunner::default();

    // Step 1: Login to get cookie
    let req1 = RequestSpec {
        url: format!("{}/login", mock_server.uri()),
        method: "GET".to_string(),
        ..Default::default()
    };
    let res1 = runner.execute(&req1, None, &Default::default()).await.unwrap();
    assert_eq!(res1.status, 200);

    // Step 2: Profile request using same runner should automatically send the cookie
    let req2 = RequestSpec {
        url: format!("{}/profile", mock_server.uri()),
        method: "GET".to_string(),
        ..Default::default()
    };
    let res2 = runner.execute(&req2, None, &Default::default()).await.unwrap();
    assert_eq!(res2.status, 200);
    assert_eq!(res2.body, "User profile");
}
