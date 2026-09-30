use super::*;

#[tokio::test]
async fn neovim_page_requires_auth_and_an_enabled_project() {
    let config = BootConfig::default();
    let mut fixture = demo_fixture(Vec::new());
    fixture.projects.push(ProjectFixture {
        name: "disabled".into(),
        enabled: false,
        project_dir: PathBuf::from("."),
        deployments: Vec::new(),
    });
    let state = test_state_with_fixture(config.clone(), fixture).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let response = public_response(
        state.clone(),
        Request::builder()
            .uri("/demo/_neovim")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    for (path, expected) in [
        ("/demo/_neovim", StatusCode::OK),
        ("/demo/_neovim/", StatusCode::OK),
        ("/missing/_neovim", StatusCode::NOT_FOUND),
        ("/disabled/_neovim", StatusCode::NOT_FOUND),
    ] {
        let response = public_response(
            state.clone(),
            Request::builder()
                .uri(path)
                .header(header::COOKIE, format!("{AUTH_COOKIE_NAME}={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), expected);
        if expected == StatusCode::OK {
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let html = std::str::from_utf8(&body).unwrap();
            assert!(html.contains("data-neovim-canvas"));
            assert!(html.contains("/demo/_neovim/ws"));
            assert!(!html.contains("terminal-viewer"));
        }
    }
    let response = public_response(
        state,
        Request::builder()
            .uri("/demo/_neovim/ws")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn editor_preference_routes_without_changing_encoded_paths() {
    let config = BootConfig::default();
    let state = test_state_with_fixture(config.clone(), demo_fixture(Vec::new())).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let query = "path=modules%2Fwith+space%2F100%25.txt";
    for (preference, expected) in [
        ("", "_files"),
        ("neovim", "_neovim"),
        ("files", "_files"),
        ("invalid", "_files"),
    ] {
        let response = public_response(
            state.clone(),
            Request::builder()
                .uri(format!("/demo/_editor?{query}"))
                .header(
                    header::COOKIE,
                    format!("{AUTH_COOKIE_NAME}={token}; latitude_editor={preference}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            &format!("/demo/{expected}?{query}")
        );
    }
    let response = public_response(
        state,
        Request::builder()
            .uri("/demo/_editor")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
