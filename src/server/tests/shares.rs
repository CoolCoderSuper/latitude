use super::*;

#[tokio::test]
async fn public_api_manages_deployment_shares() {
    let state = test_state_with_fixture(
        BootConfig::default(),
        demo_fixture(vec![fixture_static(
            "website",
            PathBuf::from("."),
            "index.html",
        )]),
    )
    .await;
    let response = public_api_create_share(
        State(state.clone()),
        axum::Json(CreateDeploymentShareRequest {
            project: "demo".to_string(),
            deployment: "website".to_string(),
            password: Some("review-only".to_string()),
            expires_at: None,
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let share_token = created["token"].as_str().unwrap().to_string();
    assert_eq!(created["has_password"], true);
    assert!(created.get("password").is_none());

    let response = public_api_list_shares(State(state.clone())).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let shares: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(shares.as_array().unwrap().len(), 1);
    assert_eq!(shares[0]["deployment"], "website");
    assert!(shares[0].get("password").is_none());

    let response =
        public_api_delete_share(axum::extract::Path(share_token), State(state.clone())).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(state.catalog().list_shares().await.unwrap().is_empty());
}

#[tokio::test]
async fn share_ui_exchanges_html_fragments() {
    let state = test_state_with_fixture(
        BootConfig::default(),
        demo_fixture(vec![fixture_static(
            "website",
            PathBuf::from("."),
            "index.html",
        )]),
    )
    .await;
    let response = public_ui_get_shares(
        axum::extract::Path(("demo".to_string(), "website".to_string())),
        State(state.clone()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/html; charset=utf-8")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();
    assert!(rendered.contains("hx-post=\"/__latitude/ui/shares/demo/website\""));
    assert!(rendered.contains("No links yet"));

    let response = public_ui_create_share(
        axum::extract::Path(("demo".to_string(), "website".to_string())),
        State(state.clone()),
        axum::extract::Form(ShareUiForm {
            password: Some("review-only".to_string()),
            expiry: Some(3600),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();
    assert!(rendered.contains("Share link created."));
    assert!(rendered.contains("Password protected"));
    assert!(rendered.contains("hx-delete="));

    let shares = state.catalog().list_shares().await.unwrap();
    assert_eq!(shares.len(), 1);
    let share_token = shares[0].token.clone();
    let response = public_ui_delete_share(
        axum::extract::Path(("demo".to_string(), "website".to_string(), share_token)),
        State(state.clone()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();
    assert!(rendered.contains("Share link revoked."));
    assert!(state.catalog().list_shares().await.unwrap().is_empty());
}

#[test]
fn renders_share_dialog_as_htmx_controls() {
    let shares = vec![DeploymentShareConfig {
        token: "open123".to_string(),
        project: "demo".to_string(),
        deployment: "website".to_string(),
        password: None,
        expires_at: None,
    }];

    let rendered = render_share_dialog_shell("demo", "website", &shares, None).into_string();

    assert!(rendered.contains("data-share-dialog-shell"));
    assert!(rendered.contains("hx-post=\"/__latitude/ui/shares/demo/website\""));
    assert!(rendered.contains("hx-delete=\"/__latitude/ui/shares/demo/website/open123\""));
    assert!(rendered.contains("data-share-url=\"/__latitude/share/open123/\""));
    assert!(!rendered.contains("fetch("));
}

#[tokio::test]
async fn public_share_management_requires_authentication() {
    let state = test_state(BootConfig::default()).await;
    let response = public_router(state)
        .oneshot(
            Request::builder()
                .uri(PUBLIC_API_SHARES_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn serves_unprotected_deployment_share_without_public_auth() {
    let seed = demo_fixture_with_shares(
        vec![fixture_page(
            "report",
            "# Shared Report",
            PageFormat::Markdown,
            None,
            None,
        )],
        vec![DeploymentShareConfig {
            token: "open123".to_string(),
            project: "demo".to_string(),
            deployment: "report".to_string(),
            password: None,
            expires_at: None,
        }],
    );
    let state = test_state_with_fixture(BootConfig::default(), seed).await;
    let req = Request::builder()
        .uri("/__latitude/share/open123/")
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();

    assert!(rendered.contains("<h1>Shared Report</h1>"));
    assert!(!rendered.contains("Sign in to"));
}

#[tokio::test]
async fn password_protected_deployment_share_sets_scoped_cookie() {
    let seed = demo_fixture_with_shares(
        vec![fixture_page(
            "report",
            "# Locked Report",
            PageFormat::Markdown,
            None,
            None,
        )],
        vec![DeploymentShareConfig {
            token: "locked123".to_string(),
            project: "demo".to_string(),
            deployment: "report".to_string(),
            password: Some("secret".to_string()),
            expires_at: None,
        }],
    );
    let state = test_state_with_fixture(BootConfig::default(), seed).await;
    let req = Request::builder()
        .uri("/__latitude/share/locked123/")
        .body(Body::empty())
        .unwrap();

    let response = public_response(state.clone(), req).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();
    assert!(rendered.contains("Open shared deployment"));

    let req = Request::builder()
        .method("POST")
        .uri("/__latitude/share/locked123/")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(
            "password=secret&next=%2F__latitude%2Fshare%2Flocked123%2F",
        ))
        .unwrap();
    let response = public_response(state.clone(), req).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/__latitude/share/locked123/")
    );
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("latitude_share_locked123="));
    assert!(cookie.contains("Path=/__latitude/share/locked123"));

    let req = Request::builder()
        .uri("/__latitude/share/locked123/")
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    let response = public_response(state, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();

    assert!(rendered.contains("<h1>Locked Report</h1>"));
}

#[tokio::test]
async fn expired_deployment_share_returns_gone() {
    let seed = demo_fixture_with_shares(
        vec![fixture_page(
            "report",
            "# Old Report",
            PageFormat::Markdown,
            None,
            None,
        )],
        vec![DeploymentShareConfig {
            token: "expired123".to_string(),
            project: "demo".to_string(),
            deployment: "report".to_string(),
            password: None,
            expires_at: Some(1),
        }],
    );
    let state = test_state_with_fixture(BootConfig::default(), seed).await;
    let req = Request::builder()
        .uri("/__latitude/share/expired123/")
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
    assert_eq!(response.status(), StatusCode::GONE);
}
