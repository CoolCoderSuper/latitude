use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use axum::{
    body::{Body, to_bytes},
    extract::State,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::IntoResponse,
};

use crate::{
    config::{
        ApplicationConfig, ApplicationTarget, BootConfig, CatalogSeed, DeploymentShareConfig,
        DesktopConfig, DesktopMode, PageFormat, ProjectConfig, SeedApplicationConfig,
        SeedApplicationTarget, SeedProjectConfig, T3CodeConfig, decode_page_binary_content,
        encode_page_binary_content,
    },
    desktop::DesktopInfoResponse,
    state::AppState,
    storage::CatalogStore,
};

use super::{
    assets::{embedded_asset_names, public_asset},
    auth::{clean_next_path, public_password_matches, public_request_is_authenticated},
    command::{
        CreateDeploymentShareRequest, get_config, get_project_deployment, get_project_page_content,
    },
    constants::{AUTH_COOKIE_NAME, LATITUDE_THEME_COOKIE, LOGIN_PATH},
    files_api::public_ui_put_project_file,
    git::{
        GitAction, GitDiffReport, GitFileChange, GitFileDiff, GitStatusSummary,
        parse_diff_file_sections, parse_git_action_form, parse_porcelain_status,
        parse_public_git_action_payload, public_diff_response,
    },
    page::{
        page_theme_from_headers, parse_page_payload, render_page_content,
        render_project_page_content,
    },
    paths::{
        ProjectPath, display_path, filtered_cookie_header, join_upstream_url, resolve_project_path,
        sanitized_relative_path, split_project_path,
    },
    public::{
        ShareUiForm, public_api_create_share, public_api_delete_share, public_api_list_shares,
        public_entry, public_project_detail, public_ui_create_share, public_ui_delete_share,
        public_ui_get_shares,
    },
    render::{
        diff_line_class, highlight_diff_lines, render_diff_code_output, render_diff_file_update,
        render_diff_workspace_fragment, render_project_diff, render_project_files,
        render_project_home, render_project_terminal, render_root_desktop, render_root_terminal,
        render_server_home, render_share_dialog_shell, syntax_name_for_path,
    },
    terminal_api::{PublicTerminalInfoResponse, parse_terminal_command_payload},
};

const TEST_HOSTNAME: &str = "test-host";

async fn test_state(config: BootConfig) -> AppState {
    test_state_with_seed(config, CatalogSeed::default()).await
}

async fn test_state_with_seed(config: BootConfig, seed: CatalogSeed) -> AppState {
    let data_dir = std::env::temp_dir().join(format!(
        "latitude-test-data-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let catalog = CatalogStore::open_for_tests(data_dir).await.unwrap();
    catalog.import_config_seed_if_needed(&seed).await.unwrap();
    AppState::new(PathBuf::from("latitude.test.json"), config, catalog)
}

fn demo_seed(deployments: Vec<SeedApplicationConfig>) -> CatalogSeed {
    demo_seed_with_shares(deployments, Vec::new())
}

fn demo_seed_with_shares(
    deployments: Vec<SeedApplicationConfig>,
    share_links: Vec<DeploymentShareConfig>,
) -> CatalogSeed {
    CatalogSeed {
        share_links,
        projects: vec![SeedProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments,
        }],
    }
}

fn seed_page(
    name: &str,
    content: &str,
    format: PageFormat,
    media_type: Option<&str>,
    title: Option<&str>,
) -> SeedApplicationConfig {
    SeedApplicationConfig {
        name: name.to_string(),
        enabled: true,
        target: SeedApplicationTarget::Page {
            content: content.to_string(),
            format,
            media_type: media_type.map(str::to_string),
            title: title.map(str::to_string),
        },
    }
}

fn seed_static(name: &str, root: PathBuf, index_file: &str) -> SeedApplicationConfig {
    SeedApplicationConfig {
        name: name.to_string(),
        enabled: true,
        target: SeedApplicationTarget::Static {
            root,
            index_file: index_file.to_string(),
            spa_fallback: false,
        },
    }
}

#[test]
fn splits_project_home_and_deployment_paths() {
    assert_eq!(
        split_project_path("/demo/website1/about"),
        Some(ProjectPath::Deployment {
            project: "demo".to_string(),
            deployment: "website1".to_string(),
            remainder: "/about".to_string()
        })
    );
    assert_eq!(
        split_project_path("/demo/website1"),
        Some(ProjectPath::Deployment {
            project: "demo".to_string(),
            deployment: "website1".to_string(),
            remainder: "/".to_string()
        })
    );
    assert_eq!(
        split_project_path("/demo"),
        Some(ProjectPath::Project {
            project: "demo".to_string()
        })
    );
    assert_eq!(
        split_project_path("/demo/"),
        Some(ProjectPath::Project {
            project: "demo".to_string()
        })
    );
    assert_eq!(split_project_path("/demo//website1"), None);
    assert_eq!(split_project_path("/"), None);
}

#[test]
fn resolves_relative_paths_against_project_dir() {
    assert_eq!(
        resolve_project_path(Path::new("projects/demo"), Path::new("dist")),
        PathBuf::from("projects/demo").join("dist")
    );
}

#[test]
fn rejects_path_traversal_for_static_files() {
    assert!(sanitized_relative_path("/assets/app.js").is_some());
    assert!(sanitized_relative_path("/../secret.txt").is_none());
    assert!(sanitized_relative_path("/%2e%2e/secret.txt").is_none());
    assert!(sanitized_relative_path("/nested%2fsecret.txt").is_none());
}

#[tokio::test]
async fn authenticates_public_requests_with_signed_cookie() {
    let config = BootConfig::default();
    let state = test_state(config.clone()).await;
    let cookie = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .header(header::COOKIE, format!("{AUTH_COOKIE_NAME}={cookie}"))
        .body(Body::empty())
        .unwrap();

    assert!(public_request_is_authenticated(&state, &config, &req));

    let changed_config = BootConfig {
        public_password: "changed".to_string(),
        ..config
    };
    assert!(!public_request_is_authenticated(
        &state,
        &changed_config,
        &req
    ));
}

#[tokio::test]
async fn authenticates_public_requests_with_bearer_token() {
    let config = BootConfig::default();
    let state = test_state(config.clone()).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    assert!(public_request_is_authenticated(&state, &config, &req));
}

#[tokio::test]
async fn authenticated_public_api_manages_deployment_shares() {
    let config = BootConfig::default();
    let state = test_state_with_seed(
        config.clone(),
        demo_seed(vec![seed_static(
            "website",
            PathBuf::from("."),
            "index.html",
        )]),
    )
    .await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );

    let response = public_api_create_share(
        State(state.clone()),
        headers,
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

    let request = Request::builder()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = public_api_list_shares(State(state.clone()), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let shares: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(shares.as_array().unwrap().len(), 1);
    assert_eq!(shares[0]["deployment"], "website");
    assert!(shares[0].get("password").is_none());

    let request = Request::builder()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = public_api_delete_share(
        axum::extract::Path(share_token),
        State(state.clone()),
        request,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(state.catalog().list_shares().await.unwrap().is_empty());
}

#[tokio::test]
async fn authenticated_share_ui_exchanges_html_fragments() {
    let config = BootConfig::default();
    let state = test_state_with_seed(
        config.clone(),
        demo_seed(vec![seed_static(
            "website",
            PathBuf::from("."),
            "index.html",
        )]),
    )
    .await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );

    let response = public_ui_get_shares(
        axum::extract::Path(("demo".to_string(), "website".to_string())),
        State(state.clone()),
        headers.clone(),
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
        headers.clone(),
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
        headers,
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
    let request = Request::builder().body(Body::empty()).unwrap();

    let response = public_api_list_shares(State(state), request).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn cleans_public_login_next_paths() {
    assert_eq!(
        clean_next_path(Some("/demo/_diff?x=1".to_string())),
        "/demo/_diff?x=1"
    );
    assert_eq!(
        clean_next_path(Some("https://example.com".to_string())),
        "/"
    );
    assert_eq!(clean_next_path(Some("//example.com".to_string())), "/");
    assert_eq!(clean_next_path(Some(LOGIN_PATH.to_string())), "/");
    assert_eq!(clean_next_path(Some("/demo name".to_string())), "/");
}

#[test]
fn filters_public_auth_cookie_from_proxy_headers() {
    let value = HeaderValue::from_static("app=one; latitude_public_session=secret; theme=dark");

    assert_eq!(
        filtered_cookie_header(&value, AUTH_COOKIE_NAME).as_deref(),
        Some("app=one; theme=dark")
    );

    let value = HeaderValue::from_static("latitude_public_session=secret");
    assert_eq!(filtered_cookie_header(&value, AUTH_COOKIE_NAME), None);
}

#[test]
fn matches_public_passwords_exactly() {
    assert!(public_password_matches("test", "test"));
    assert!(!public_password_matches("test", "Test"));
    assert!(!public_password_matches("test", "test "));
}

#[test]
fn reads_page_theme_from_cookie() {
    let req = Request::builder()
        .header(header::COOKIE, format!("{LATITUDE_THEME_COOKIE}=dark"))
        .body(Body::empty())
        .unwrap();

    assert_eq!(page_theme_from_headers(req.headers()), Some("dark"));
}

#[test]
fn generated_theme_assets_do_not_follow_system_color_scheme() {
    let styles = [
        ("auth", include_str!("assets/auth.css")),
        ("project home", include_str!("assets/project-home.css")),
        ("diff viewer", include_str!("assets/diff-viewer.css")),
        (
            "terminal viewer",
            include_str!("assets/terminal-viewer.css"),
        ),
        ("desktop viewer", include_str!("assets/desktop-viewer.css")),
        ("page", include_str!("assets/page.css")),
        ("common theme", include_str!("assets/common-theme.css")),
    ];

    for (name, style) in styles {
        assert!(
            !style.contains("prefers-color-scheme"),
            "{name} style should use the Latitude theme toggle, not system color scheme"
        );
        assert!(
            !style.contains("color-scheme: light dark"),
            "{name} style should not opt back into automatic system theming"
        );
    }

    let rendered = render_server_home(&BootConfig::default(), &[], &HashMap::new(), TEST_HOSTNAME);
    assert!(!rendered.contains("prefers-color-scheme"));
    assert!(!rendered.contains("matchMedia('(prefers-color-scheme"));
    assert!(rendered.contains("src=\"/__latitude/assets/theme-bootstrap.js?v=2\""));
    assert!(rendered.contains("src=\"/__latitude/assets/theme-toggle.js?v=2\""));
    assert!(!rendered.contains("var cookieName"));
}

#[tokio::test]
async fn serves_embedded_assets_with_cache_validation() {
    assert!(embedded_asset_names().contains(&"htmx.min.js"));
    let response = public_asset(
        axum::extract::Path("htmx.min.js".to_string()),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("public, no-cache")
    );
    let etag = response.headers().get(header::ETAG).unwrap().clone();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.starts_with(b"var htmx="));

    let mut headers = HeaderMap::new();
    headers.insert(header::IF_NONE_MATCH, etag);
    let response = public_asset(axum::extract::Path("htmx.min.js".to_string()), headers).await;
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.is_empty());
}

#[test]
fn joins_upstream_url_with_query() {
    let joined = join_upstream_url("http://127.0.0.1:3000", "/hello", Some("a=1")).unwrap();
    assert_eq!(joined, "http://127.0.0.1:3000/hello?a=1");
}

#[test]
fn joins_upstream_url_with_base_path() {
    let joined = join_upstream_url("http://127.0.0.1:3000/base/", "/hello", Some("a=1")).unwrap();
    assert_eq!(joined, "http://127.0.0.1:3000/base/hello?a=1");
}

#[test]
fn parses_raw_markdown_page_payload() {
    let payload =
        parse_page_payload(Some("text/markdown; charset=utf-8"), b"# Agent Report").unwrap();

    assert_eq!(payload.format, PageFormat::Markdown);
    assert_eq!(payload.content, "# Agent Report");
    assert_eq!(payload.title, None);
}

#[test]
fn parses_json_page_payload() {
    let payload = parse_page_payload(
        Some("application/json"),
        br##"{"title":"Report","format":"markdown","content":"# Done"}"##,
    )
    .unwrap();

    assert_eq!(payload.format, PageFormat::Markdown);
    assert_eq!(payload.content, "# Done");
    assert_eq!(payload.title.as_deref(), Some("Report"));
}

#[test]
fn parses_raw_image_page_payload() {
    let payload = parse_page_payload(Some("image/png"), b"\x89PNG\r\n").unwrap();

    assert_eq!(payload.format, PageFormat::Binary);
    assert_eq!(payload.media_type.as_deref(), Some("image/png"));
    assert_eq!(
        decode_page_binary_content(&payload.content).unwrap(),
        b"\x89PNG\r\n"
    );
    assert_eq!(payload.title, None);
}

#[test]
fn parses_raw_video_page_payload() {
    let payload = parse_page_payload(Some("video/mp4"), b"mp4 bytes").unwrap();

    assert_eq!(payload.format, PageFormat::Binary);
    assert_eq!(payload.media_type.as_deref(), Some("video/mp4"));
    assert_eq!(
        decode_page_binary_content(&payload.content).unwrap(),
        b"mp4 bytes"
    );
}

#[test]
fn infers_html_for_raw_html_payload() {
    let payload = parse_page_payload(None, b"<section><h1>Hello</h1></section>").unwrap();

    assert_eq!(payload.format, PageFormat::Html);
}

#[tokio::test]
async fn command_config_response_is_boot_only() {
    let seed = CatalogSeed {
        projects: vec![SeedProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: Vec::new(),
        }],
        share_links: vec![DeploymentShareConfig {
            token: "abc123".to_string(),
            project: "demo".to_string(),
            deployment: "missing".to_string(),
            password: None,
            expires_at: None,
        }],
        ..CatalogSeed::default()
    };
    let state = test_state_with_seed(BootConfig::default(), seed).await;

    let response = get_config(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(value.get("public_bind").is_some());
    assert!(value.get("projects").is_none());
    assert!(value.get("share_links").is_none());
}

#[tokio::test]
async fn command_deployment_response_omits_page_content_and_content_endpoint_returns_bytes() {
    let seed = CatalogSeed {
        projects: vec![SeedProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![SeedApplicationConfig {
                name: "report".to_string(),
                enabled: true,
                target: SeedApplicationTarget::Page {
                    content: "# Report".to_string(),
                    format: PageFormat::Markdown,
                    media_type: None,
                    title: Some("Report".to_string()),
                },
            }],
        }],
        ..CatalogSeed::default()
    };
    let state = test_state_with_seed(BootConfig::default(), seed).await;

    let response = get_project_deployment(
        axum::extract::Path(("demo".to_string(), "report".to_string())),
        State(state.clone()),
    )
    .await
    .unwrap()
    .into_response();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["kind"], "page");
    assert_eq!(value["title"], "Report");
    assert!(value.get("content").is_none());

    let response = get_project_page_content(
        axum::extract::Path(("demo".to_string(), "report".to_string())),
        State(state),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/markdown; charset=utf-8")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"# Report");
}

#[test]
fn renders_markdown_as_html_document() {
    let rendered = render_page_content(
        None,
        PageFormat::Markdown,
        "# Agent Report\n\n- Done",
        Some("dark"),
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("<html lang=\"en\" data-latitude-theme=\"dark\">"));
    assert!(rendered.contains("<title>Agent Report - test-host</title>"));
    assert!(rendered.contains("data-latitude-theme-toggle"));
    assert!(rendered.contains("<h1>Agent Report</h1>"));
    assert!(rendered.contains("<li>Done</li>"));
}

#[test]
fn renders_project_markdown_document_with_back_to_project_shell() {
    let rendered = render_project_page_content(
        "demo",
        None,
        PageFormat::Markdown,
        None,
        "# Agent Report\n\n- Done",
        Some("dark"),
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("<html lang=\"en\" data-latitude-theme=\"dark\">"));
    assert!(rendered.contains("<title>Agent Report - test-host</title>"));
    assert!(rendered.contains("href=\"/demo\">Back to project</a>"));
    assert!(rendered.contains("<p class=\"latitude-page-hostname\">demo on test-host</p>"));
    assert!(rendered.contains("data-latitude-theme-toggle"));
    assert!(!rendered.contains("data-latitude-theme-switcher"));
    assert!(!rendered.contains("data-latitude-theme-button"));
    assert!(rendered.contains("<h1>Agent Report</h1>"));
}

#[test]
fn renders_video_page_document_with_back_to_project_shell() {
    let rendered = render_project_page_content(
        "demo",
        Some("Launch Clip"),
        PageFormat::Binary,
        Some("video/mp4"),
        "",
        Some("light"),
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("<html lang=\"en\" data-latitude-theme=\"light\">"));
    assert!(rendered.contains("<title>Launch Clip - test-host</title>"));
    assert!(rendered.contains("href=\"/demo\">Back to project</a>"));
    assert!(rendered.contains("<video controls preload=\"metadata\" src=\"?raw=1\">"));
}

#[test]
fn renders_project_home_with_enabled_deployments() {
    let rendered = render_project_home(
        &ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![
                ApplicationConfig {
                    name: "website".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Static {
                        root: PathBuf::from("."),
                        index_file: "index.html".to_string(),
                        spa_fallback: true,
                    },
                },
                ApplicationConfig {
                    name: "report".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        format: PageFormat::Markdown,
                        media_type: None,
                        title: Some("Weekly Report".to_string()),
                    },
                },
                ApplicationConfig {
                    name: "draft".to_string(),
                    enabled: false,
                    target: ApplicationTarget::Page {
                        format: PageFormat::Markdown,
                        media_type: None,
                        title: None,
                    },
                },
            ],
        },
        &GitStatusSummary::default(),
        true,
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("<title>demo - Latitude Project - test-host</title>"));
    assert!(rendered.contains("href=\"/\">Back to projects</a>"));
    assert!(rendered.contains("Project tools and deployments on test-host"));
    assert!(rendered.contains("href=\"/demo/_diff\""));
    assert!(rendered.contains("Code changes"));
    assert!(rendered.contains("href=\"/demo/_terminal\""));
    assert!(rendered.contains("Run commands in the project directory"));
    assert!(
        rendered.contains("href=\"/__latitude/t3code/demo\" target=\"_blank\" rel=\"noopener\"")
    );
    assert!(rendered.contains("Open in T3 Code"));
    assert!(rendered.contains("href=\"/demo/website\""));
    assert!(rendered.contains("Static website"));
    assert!(rendered.contains("href=\"/demo/report\""));
    assert!(rendered.contains("Page: Weekly Report"));
    assert!(rendered.contains("data-project-shell data-project=\"demo\""));
    assert!(rendered.contains("data-deployment=\"website\""));
    assert!(rendered.contains("aria-label=\"Manage shares for report\""));
    assert!(rendered.contains("data-share-dialog"));
    assert!(rendered.contains("hx-get=\"/__latitude/ui/shares/demo/website\""));
    assert!(rendered.contains("hx-target=\"[data-share-dialog-shell]\""));
    assert!(rendered.contains("src=\"/__latitude/assets/project-home.js?v=2\""));
    assert!(!rendered.contains("/__latitude/api/shares"));
    assert!(!rendered.contains("/demo/draft"));
    assert!(!rendered.contains("data-deployment=\"draft\""));
}

#[tokio::test]
async fn serves_binary_page_document_shell_by_default() {
    let config = BootConfig::default();
    let seed = demo_seed(vec![seed_page(
        "snapshot",
        &encode_page_binary_content(b"png bytes"),
        PageFormat::Binary,
        Some("image/png"),
        None,
    )]);
    let state = test_state_with_seed(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
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

    assert!(rendered.contains("href=\"/demo\">Back to project</a>"));
    assert!(rendered.contains("<img src=\"?raw=1\" alt=\"Latitude Page\">"));
    assert!(!rendered.contains("png bytes"));
}

#[tokio::test]
async fn serves_binary_page_document_shell_for_media_accept_requests() {
    let config = BootConfig::default();
    let seed = demo_seed(vec![seed_page(
        "snapshot",
        &encode_page_binary_content(b"png bytes"),
        PageFormat::Binary,
        Some("image/png"),
        Some("Build Snapshot"),
    )]);
    let state = test_state_with_seed(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "image/avif,image/webp,image/png,*/*;q=0.8")
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
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

    assert!(rendered.contains("href=\"/demo\">Back to project</a>"));
    assert!(rendered.contains("<img src=\"?raw=1\" alt=\"Build Snapshot\">"));
    assert!(!rendered.contains("png bytes"));
}

#[tokio::test]
async fn serves_binary_page_document_raw_query_with_media_type() {
    let config = BootConfig::default();
    let seed = demo_seed(vec![seed_page(
        "snapshot",
        &encode_page_binary_content(b"png bytes"),
        PageFormat::Binary,
        Some("image/png"),
        Some("Build Snapshot"),
    )]);
    let state = test_state_with_seed(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot?raw=1")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "text/html")
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    assert_eq!(&body[..], b"png bytes");
}

#[tokio::test]
async fn serves_static_media_document_shell_by_default() {
    let root = std::env::temp_dir().join(format!(
        "latitude-static-media-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("snapshot.png"), b"png bytes").unwrap();

    let config = BootConfig::default();
    let seed = demo_seed(vec![seed_static("snapshot", root.clone(), "snapshot.png")]);
    let state = test_state_with_seed(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
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

    assert!(rendered.contains("href=\"/demo\">Back to project</a>"));
    assert!(rendered.contains("<img src=\"?raw=1\" alt=\"snapshot\">"));
    assert!(!rendered.contains("png bytes"));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn serves_static_media_document_raw_query_with_media_type() {
    let root = std::env::temp_dir().join(format!(
        "latitude-static-media-raw-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("snapshot.png"), b"png bytes").unwrap();

    let config = BootConfig::default();
    let seed = demo_seed(vec![seed_static("snapshot", root.clone(), "snapshot.png")]);
    let state = test_state_with_seed(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot?raw=1")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    assert_eq!(&body[..], b"png bytes");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn serves_static_site_without_document_shell() {
    let root = std::env::temp_dir().join(format!(
        "latitude-static-site-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("index.html"), b"<!doctype html><h1>Site</h1>").unwrap();

    let config = BootConfig::default();
    let seed = demo_seed(vec![seed_static("website", root.clone(), "index.html")]);
    let state = test_state_with_seed(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/website")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/html")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(rendered, "<!doctype html><h1>Site</h1>");
    assert!(!rendered.contains("Back to project"));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn serves_unprotected_deployment_share_without_public_auth() {
    let seed = demo_seed_with_shares(
        vec![seed_page(
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
    let state = test_state_with_seed(BootConfig::default(), seed).await;
    let req = Request::builder()
        .uri("/__latitude/share/open123/")
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();

    assert!(rendered.contains("<h1>Shared Report</h1>"));
    assert!(!rendered.contains("Sign in to"));
}

#[tokio::test]
async fn password_protected_deployment_share_sets_scoped_cookie() {
    let seed = demo_seed_with_shares(
        vec![seed_page(
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
    let state = test_state_with_seed(BootConfig::default(), seed).await;
    let req = Request::builder()
        .uri("/__latitude/share/locked123/")
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state.clone()), req).await;
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
    let response = public_entry(State(state.clone()), req).await;
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
    let response = public_entry(State(state), req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();

    assert!(rendered.contains("<h1>Locked Report</h1>"));
}

#[tokio::test]
async fn expired_deployment_share_returns_gone() {
    let seed = demo_seed_with_shares(
        vec![seed_page(
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
    let state = test_state_with_seed(BootConfig::default(), seed).await;
    let req = Request::builder()
        .uri("/__latitude/share/expired123/")
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
    assert_eq!(response.status(), StatusCode::GONE);
}

#[test]
fn renders_project_diff_with_escaped_highlighted_lines() {
    let project = ProjectConfig {
        name: "demo".to_string(),
        enabled: true,
        project_dir: PathBuf::from("."),
        deployments: Vec::new(),
    };
    let report = GitDiffReport {
        repo_dir: PathBuf::from("C:/work/demo"),
        status: GitStatusSummary::default(),
        file_changes: vec![
            GitFileChange {
                path: "src/server.rs".to_string(),
                original_path: None,
                index_status: ' ',
                worktree_status: 'M',
                diffs: vec![GitFileDiff {
                    label: "Unstaged".to_string(),
                    command: "git diff --no-ext-diff --color=never".to_string(),
                    path: "src/server.rs".to_string(),
                    content:
                        "diff --git a/src/server.rs b/src/server.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 42;"
                            .to_string(),
                }],
            },
            GitFileChange {
                path: "src/new.rs".to_string(),
                original_path: None,
                index_status: 'A',
                worktree_status: ' ',
                diffs: Vec::new(),
            },
        ],
    };
    let rendered = render_project_diff(&project, &report, TEST_HOSTNAME);

    assert!(rendered.contains("<title>demo code changes - Latitude - test-host</title>"));
    assert!(rendered.contains("href=\"/demo\""));
    assert!(rendered.contains("<p>demo on test-host</p>"));
    assert!(rendered.contains("<h2>Unstaged files</h2>"));
    assert!(rendered.contains("<h2>Staged files</h2>"));
    assert!(rendered.contains("data-diff-workspace"));
    assert!(rendered.contains("data-action-url=\"/demo/_diff\""));
    assert!(!rendered.contains("hx-sync="));
    assert!(rendered.contains("hx-swap=\"none\""));
    assert!(rendered.contains("data-file-panel=\"unstaged\""));
    assert!(rendered.contains(
        "<details class=\"file-card\" data-file-section=\"unstaged\" data-file-path=\"src/server.rs\">"
    ));
    assert!(rendered.contains("data-git-action=\"stage_all\""));
    assert!(rendered.contains("data-git-action=\"discard_all\""));
    assert!(rendered.contains("data-git-action=\"stage_file\""));
    assert!(rendered.contains("data-git-action=\"discard_file\""));
    assert!(rendered.contains("data-path=\"src/server.rs\""));
    assert!(rendered.contains("hx-confirm=\"Discard all unstaged changes"));
    let file_summary_start = rendered
        .find("<summary class=\"file-summary\">")
        .expect("file summary should render");
    let file_content_start = rendered[file_summary_start..]
        .find("<div class=\"file-content\">")
        .map(|offset| file_summary_start + offset)
        .expect("file content should render after summary");
    let file_summary = &rendered[file_summary_start..file_content_start];
    assert!(file_summary.contains("data-git-action=\"stage_file\""));
    assert!(file_summary.contains("data-git-action=\"discard_file\""));
    assert!(!rendered.contains("class=\"file-diff-title\""));
    assert!(!rendered.contains("<strong>Unstaged</strong>"));
    assert!(!rendered.contains("class=\"file-count\""));
    assert!(!rendered.contains(">1 diff<"));
    assert!(!rendered.contains("git diff --no-ext-diff --color=never"));
    assert!(rendered.contains("data-git-action=\"unstage_file\""));
    assert!(rendered.contains("data-path=\"src/new.rs\""));
    assert!(rendered.contains("data-commit-message"));
    assert!(rendered.contains("Commit staged"));
    assert!(rendered.contains("data-git-action=\"pull\""));
    assert!(rendered.contains("href=\"/demo/_diff/history\""));
    assert!(rendered.contains("hx-patch=\"/demo/_diff\""));
    assert!(rendered.contains("src=\"/__latitude/assets/diff-viewer.js?v=2\""));
    assert!(!rendered.contains("method=\"post\""));
    assert!(!rendered.contains("Done."));
    assert!(rendered.contains("class=\"line remove\">-<span class=\"tok-keyword\">let</span> old"));
    assert!(rendered.contains("class=\"line add\">+<span class=\"tok-keyword\">let</span> new"));
    assert!(rendered.contains("<span class=\"tok-number\">42</span>"));
    assert!(!rendered.contains("<h2>Git status</h2>"));
    assert!(!rendered.contains("<h2>Untracked files</h2>"));
}

#[test]
fn renders_diff_workspace_fragment_without_full_document() {
    let report = GitDiffReport {
        repo_dir: PathBuf::from("C:/work/demo"),
        status: GitStatusSummary::default(),
        file_changes: vec![GitFileChange {
            path: "README.md".to_string(),
            original_path: None,
            index_status: '?',
            worktree_status: '?',
            diffs: Vec::new(),
        }],
    };

    let rendered = render_diff_workspace_fragment(&report, "/demo/_diff").into_string();

    assert!(rendered.contains("data-action-status hidden"));
    assert!(rendered.contains("<h2>Unstaged files</h2>"));
    assert!(rendered.contains("data-git-action=\"stage_file\""));
    assert!(rendered.contains("data-git-action=\"discard_file\""));
    assert!(!rendered.contains("<!doctype html>"));
    assert!(!rendered.contains("<script>"));
}

#[test]
fn renders_targeted_diff_file_update() {
    let report = GitDiffReport {
        repo_dir: PathBuf::from("C:/work/demo"),
        status: GitStatusSummary::default(),
        file_changes: vec![GitFileChange {
            path: "README.md".to_string(),
            original_path: None,
            index_status: 'M',
            worktree_status: ' ',
            diffs: Vec::new(),
        }],
    };

    let rendered = render_diff_file_update(&report, "README.md", "/demo/_diff").into_string();

    assert!(rendered.contains("data-diff-file-update"));
    assert!(rendered.contains("data-file-section-update=\"unstaged\""));
    assert!(rendered.contains("data-file-section-update=\"staged\""));
    assert!(rendered.contains("data-file-section=\"staged\""));
    assert!(!rendered.contains("data-file-section=\"unstaged\""));
}

#[test]
fn parses_git_action_forms() {
    assert_eq!(
        parse_git_action_form(b"action=stage_all").unwrap(),
        GitAction::StageAll
    );
    assert_eq!(
        parse_git_action_form(b"action=stage_file&path=src%2Fserver.rs").unwrap(),
        GitAction::StageFile {
            path: "src/server.rs".to_string()
        }
    );
    assert_eq!(
        parse_git_action_form(b"action=unstage_all").unwrap(),
        GitAction::UnstageAll
    );
    assert_eq!(
        parse_git_action_form(b"action=unstage_file&path=src%5Cserver.rs").unwrap(),
        GitAction::UnstageFile {
            path: "src/server.rs".to_string()
        }
    );
    assert_eq!(
        parse_git_action_form(b"action=discard_all").unwrap(),
        GitAction::DiscardAll
    );
    assert_eq!(
        parse_git_action_form(b"action=discard_file&path=src%2Fserver.rs").unwrap(),
        GitAction::DiscardFile {
            path: "src/server.rs".to_string()
        }
    );
    assert_eq!(
        parse_git_action_form(b"action=fetch").unwrap(),
        GitAction::Fetch
    );
    assert_eq!(
        parse_git_action_form(b"action=pull").unwrap(),
        GitAction::Pull
    );
    assert_eq!(
        parse_git_action_form(b"action=push").unwrap(),
        GitAction::Push
    );
    assert_eq!(
        parse_git_action_form(b"action=commit&message=Ship+diff+viewer").unwrap(),
        GitAction::Commit {
            message: "Ship diff viewer".to_string()
        }
    );
    assert!(parse_git_action_form(b"action=commit&message=%20").is_err());
    assert!(parse_git_action_form(b"action=wat").is_err());
}

#[test]
fn parses_public_git_action_json_payloads() {
    assert_eq!(
        parse_public_git_action_payload(
            Some("application/json"),
            br#"{"action":"stage_file","path":"src\\server.rs"}"#,
        )
        .unwrap(),
        GitAction::StageFile {
            path: "src/server.rs".to_string()
        }
    );
    assert_eq!(
        parse_public_git_action_payload(
            Some("application/json"),
            br#"{"action":"discard_file","path":"src\\server.rs"}"#,
        )
        .unwrap(),
        GitAction::DiscardFile {
            path: "src/server.rs".to_string()
        }
    );
    assert_eq!(
        parse_public_git_action_payload(
            Some("application/json; charset=utf-8"),
            br#"{"action":"commit","message":"Ship mobile app"}"#,
        )
        .unwrap(),
        GitAction::Commit {
            message: "Ship mobile app".to_string()
        }
    );
}

#[test]
fn parses_terminal_command_payloads() {
    assert_eq!(
        parse_terminal_command_payload(Some("application/json"), br#"{"command":" cargo test "}"#,)
            .unwrap(),
        "cargo test"
    );
    assert_eq!(
        parse_terminal_command_payload(
            Some("application/x-www-form-urlencoded"),
            b"command=Get-ChildItem",
        )
        .unwrap(),
        "Get-ChildItem"
    );
    assert!(
        parse_terminal_command_payload(Some("application/json"), br#"{"command":" "}"#).is_err()
    );
}

#[test]
fn renders_project_files_with_htmx_save_form() {
    let project = ProjectConfig {
        name: "demo".to_string(),
        enabled: true,
        project_dir: PathBuf::from("C:/work/demo"),
        deployments: Vec::new(),
    };

    let rendered = render_project_files(&project, TEST_HOSTNAME);

    assert!(rendered.contains("data-file-workspace"));
    assert!(rendered.contains("hx-put=\"/__latitude/ui/files/demo\""));
    assert!(rendered.contains("hx-target=\"[data-save-state]\""));
    assert!(rendered.contains("src=\"/__latitude/assets/file-viewer.js?v=2\""));
}

#[tokio::test]
async fn authenticated_file_ui_saves_with_html_fragment_response() {
    let project_dir = std::env::temp_dir().join(format!(
        "latitude-file-ui-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&project_dir).unwrap();
    let file_path = project_dir.join("note.txt");
    std::fs::write(&file_path, "before").unwrap();
    let seed = CatalogSeed {
        share_links: Vec::new(),
        projects: vec![SeedProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir,
            deployments: Vec::new(),
        }],
    };
    let config = BootConfig::default();
    let state = test_state_with_seed(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .method("PUT")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("path=note.txt&content=hello+from+htmx"))
        .unwrap();

    let response =
        public_ui_put_project_file(axum::extract::Path("demo".to_string()), State(state), req)
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
    assert!(rendered.contains("data-file-save-result"));
    assert!(rendered.contains("data-ok=\"true\""));
    assert_eq!(
        std::fs::read_to_string(file_path).unwrap(),
        "hello from htmx"
    );
}

#[test]
fn renders_project_terminal_page() {
    let project = ProjectConfig {
        name: "demo".to_string(),
        enabled: true,
        project_dir: PathBuf::from("."),
        deployments: Vec::new(),
    };
    let info = PublicTerminalInfoResponse {
        cwd: "C:/work/demo".to_string(),
        shell: "powershell",
        timeout_seconds: 30,
        max_output_bytes: 1024,
        sessions_href: "/__latitude/api/projects/demo/terminal/sessions".to_string(),
    };
    let rendered = render_project_terminal(&project, &info, Some("signed-token"), TEST_HOSTNAME);

    assert!(rendered.contains("<title>demo terminal - Latitude - test-host</title>"));
    assert!(rendered.contains("data-latitude-theme-toggle"));
    assert!(rendered.contains("<p>demo on test-host</p>"));
    assert!(rendered.contains("data-terminal-workspace"));
    assert!(
        rendered.contains("data-sessions-path=\"/__latitude/api/projects/demo/terminal/sessions\"")
    );
    assert!(rendered.contains("data-terminal-sessions"));
    assert!(rendered.contains("data-terminal-new"));
    assert!(rendered.contains("data-terminal-stack"));
    assert!(rendered.contains("data-ws-path=\"/demo/_terminal/ws\""));
    assert!(rendered.contains("data-ws-token=\"signed-token\""));
    assert!(rendered.contains("@xterm/xterm"));
    assert!(rendered.contains("C:/work/demo"));
}

#[test]
fn renders_root_terminal_page() {
    let info = PublicTerminalInfoResponse {
        cwd: "C:/Users/tester".to_string(),
        shell: "powershell",
        timeout_seconds: 30,
        max_output_bytes: 1024,
        sessions_href: "/__latitude/api/terminal/sessions".to_string(),
    };
    let rendered = render_root_terminal(&info, Some("signed-token"), TEST_HOSTNAME);

    assert!(rendered.contains("<title>Root terminal - Latitude - test-host</title>"));
    assert!(rendered.contains("data-latitude-theme-toggle"));
    assert!(rendered.contains("<h1>Root Terminal</h1>"));
    assert!(rendered.contains("<p>User directory on test-host</p>"));
    assert!(rendered.contains("data-sessions-path=\"/__latitude/api/terminal/sessions\""));
    assert!(rendered.contains("data-ws-path=\"/_terminal/ws\""));
    assert!(rendered.contains("data-ws-token=\"signed-token\""));
    assert!(rendered.contains("C:/Users/tester"));
}

#[test]
fn renders_root_desktop_page() {
    let info = DesktopInfoResponse {
        label: "Desktop".to_string(),
        enabled: true,
        mode: DesktopMode::External,
        managed: false,
        host: "127.0.0.1".to_string(),
        port: 5900,
        view_only: true,
        websocket_href: "/_desktop/ws".to_string(),
        screens: Vec::new(),
        resolutions: Vec::new(),
    };
    let rendered = render_root_desktop(&info, Some("signed-token"), TEST_HOSTNAME);

    assert!(rendered.contains("<title>Desktop - Latitude - test-host</title>"));
    assert!(rendered.contains("data-latitude-theme-toggle"));
    assert!(rendered.contains("<h1>Desktop</h1>"));
    assert!(rendered.contains("<p>Desktop on test-host</p>"));
    assert!(!rendered.contains("desktop-target-label"));
    assert!(rendered.contains("data-desktop-workspace"));
    assert!(rendered.contains("data-desktop-screens"));
    assert!(rendered.contains("data-desktop-resolution"));
    assert!(!rendered.contains("data-desktop-clipboard"));
    assert!(rendered.contains("data-desktop-scale"));
    assert!(rendered.contains("data-desktop-fullscreen"));
    assert!(rendered.contains("data-action-path=\"/_desktop\""));
    assert!(rendered.contains("data-ws-path=\"/_desktop/ws\""));
    assert!(rendered.contains("data-ws-token=\"signed-token\""));
    assert!(rendered.contains("data-view-only=\"true\""));
    assert!(rendered.contains("data-screen-layout=\"[]\""));
    assert!(rendered.contains("data-resolution-options=\"[]\""));
    assert!(rendered.contains("src=\"/__latitude/assets/desktop-viewer.js?v=2\""));
}

#[tokio::test]
async fn serves_root_terminal_viewer() {
    let config = BootConfig::default();
    let state = test_state(config.clone()).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/_terminal")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
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

    assert!(rendered.contains("<h1>Root Terminal</h1>"));
    assert!(rendered.contains("data-sessions-path=\"/__latitude/api/terminal/sessions\""));
    assert!(rendered.contains("data-ws-path=\"/_terminal/ws\""));
}

#[tokio::test]
async fn serves_root_desktop_viewer_when_enabled() {
    let config = BootConfig {
        desktop: DesktopConfig {
            enabled: true,
            ..DesktopConfig::default()
        },
        ..BootConfig::default()
    };
    let state = test_state(config.clone()).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/_desktop")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
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

    assert!(rendered.contains("<h1>Desktop</h1>"));
    assert!(rendered.contains("data-desktop-workspace"));
    assert!(rendered.contains("data-ws-path=\"/_desktop/ws\""));
}

#[tokio::test]
async fn root_desktop_viewer_returns_not_found_when_disabled() {
    let config = BootConfig::default();
    let state = test_state(config.clone()).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/_desktop")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_entry(State(state), req).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn parses_porcelain_status_for_file_actions() {
    let changes = parse_porcelain_status(
        b" M src/server.rs\0A  src/new.rs\0?? README.md\0R  src/new-name.rs\0src/old-name.rs\0",
    );

    assert_eq!(
        changes,
        vec![
            GitFileChange {
                path: "src/server.rs".to_string(),
                original_path: None,
                index_status: ' ',
                worktree_status: 'M',
                diffs: Vec::new(),
            },
            GitFileChange {
                path: "src/new.rs".to_string(),
                original_path: None,
                index_status: 'A',
                worktree_status: ' ',
                diffs: Vec::new(),
            },
            GitFileChange {
                path: "README.md".to_string(),
                original_path: None,
                index_status: '?',
                worktree_status: '?',
                diffs: Vec::new(),
            },
            GitFileChange {
                path: "src/new-name.rs".to_string(),
                original_path: Some("src/old-name.rs".to_string()),
                index_status: 'R',
                worktree_status: ' ',
                diffs: Vec::new(),
            },
        ]
    );
    assert!(changes[0].can_stage());
    assert!(!changes[0].can_unstage());
    assert!(!changes[1].can_stage());
    assert!(changes[1].can_unstage());
    assert!(changes[2].can_stage());
}

#[test]
fn parses_combined_diff_into_file_sections() {
    let sections = parse_diff_file_sections(
        "Unstaged",
        "git diff",
        "diff --git a/src/a.rs b/src/a.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/README.md b/README.md\n@@ -0,0 +1 @@\n+hi\n",
    );

    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].path, "src/a.rs");
    assert_eq!(sections[0].label, "Unstaged");
    assert!(sections[0].content.contains("+b"));
    assert_eq!(sections[1].path, "README.md");
    assert!(sections[1].content.contains("+hi"));
}

#[test]
fn classifies_diff_lines() {
    assert_eq!(diff_line_class("diff --git a/a b/a"), Some("file"));
    assert_eq!(diff_line_class("@@ -1 +1 @@"), Some("hunk"));
    assert_eq!(diff_line_class("+added"), Some("add"));
    assert_eq!(diff_line_class("-removed"), Some("remove"));
    assert_eq!(diff_line_class(" context"), None);
}

#[test]
fn highlights_diff_code_by_file_path() {
    let mut rendered = String::new();
    render_diff_code_output(
        &mut rendered,
        "diff --git a/src/lib.rs b/src/lib.rs\n@@ -0,0 +1 @@\n+pub fn answer() -> i32 { 42 }",
        "src/lib.rs",
    );

    assert!(rendered.contains("class=\"line file\">diff --git"));
    assert!(rendered.contains("class=\"line hunk\">@@ -0,0 +1 @@"));
    assert!(rendered.contains("+<span class=\"tok-keyword\">pub</span>"));
    assert!(rendered.contains("<span class=\"tok-keyword\">fn</span> answer"));
    assert!(rendered.contains("<span class=\"tok-type\">i32</span>"));
    assert!(rendered.contains("<span class=\"tok-number\">42</span>"));
    assert!(!rendered.contains("</span>\n<span class=\"line"));
}

#[test]
fn highlights_visual_basic_diff_by_file_path() {
    let lines = serde_json::to_value(highlight_diff_lines(
        "diff --git a/src/Program.vb b/src/Program.vb\n@@ -0,0 +1,2 @@\n+Public Sub Main()\n+Dim message As String = \"hello\"",
        "src/Program.vb",
    ))
    .unwrap();
    let tokens = lines
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|line| line["tokens"].as_array().unwrap());

    let has_token_containing = |text: &str, kind: &str| {
        tokens.clone().any(|token| {
            token["kind"] == kind
                && token["text"]
                    .as_str()
                    .is_some_and(|value| value.contains(text))
        })
    };

    assert!(has_token_containing("Public", "keyword"));
    assert!(has_token_containing("Sub", "keyword"));
    assert!(has_token_containing("String", "type"));
    assert!(has_token_containing("\"hello\"", "string"));
}

#[test]
fn public_diff_response_includes_highlighted_lines() {
    let content = "diff --git a/src/lib.rs b/src/lib.rs\n@@ -0,0 +1 @@\n+let answer: i32 = 42;";
    let response = public_diff_response(GitDiffReport {
        repo_dir: PathBuf::from("C:/work/demo"),
        status: GitStatusSummary::default(),
        file_changes: vec![GitFileChange {
            path: "src/lib.rs".to_string(),
            original_path: None,
            index_status: ' ',
            worktree_status: 'M',
            diffs: vec![GitFileDiff {
                label: "Unstaged".to_string(),
                command: "git diff --no-ext-diff --color=never".to_string(),
                path: "src/lib.rs".to_string(),
                content: content.to_string(),
            }],
        }],
    });

    let payload = serde_json::to_value(&response).unwrap();
    let diff = &payload["file_changes"][0]["diffs"][0];
    assert_eq!(diff["content"], content);
    assert_eq!(diff["lines"][0]["kind"], "file");
    assert_eq!(diff["lines"][1]["kind"], "hunk");
    assert_eq!(diff["lines"][2]["kind"], "add");

    let tokens = diff["lines"][2]["tokens"].as_array().unwrap();
    assert!(
        tokens
            .iter()
            .any(|token| token["text"] == "let" && token["kind"] == "keyword")
    );
    assert!(
        tokens
            .iter()
            .any(|token| token["text"] == "i32" && token["kind"] == "type")
    );
}

#[test]
fn detects_syntax_with_syntect_path_lookup() {
    assert_eq!(syntax_name_for_path("src/main.rs"), "Rust");
    assert_eq!(syntax_name_for_path("scripts/tool.py"), "Python");
    assert_eq!(syntax_name_for_path("package.json"), "JSON");
    assert_eq!(syntax_name_for_path("frontend/App.ts"), "TypeScript");
    assert_eq!(syntax_name_for_path("frontend/App.tsx"), "TypeScriptReact");
    assert_eq!(syntax_name_for_path("frontend/App.svelte"), "Svelte");
    assert_eq!(syntax_name_for_path("frontend/App.vue"), "Vue Component");
    assert_eq!(syntax_name_for_path("Dockerfile"), "Dockerfile");
    assert_eq!(syntax_name_for_path("config.toml"), "TOML");
    assert_eq!(syntax_name_for_path("scripts/setup.ps1"), "PowerShell");
    assert_eq!(syntax_name_for_path("src/Program.vb"), "VB.NET");
    assert_ne!(syntax_name_for_path("README.md"), "Plain Text");
    assert_eq!(
        syntax_name_for_path("unknown.latitude-example"),
        "Plain Text"
    );
}

#[test]
fn trims_windows_extended_path_prefix_for_display() {
    assert_eq!(
        display_path(Path::new(r"\\?\C:\work\demo")),
        r"C:\work\demo"
    );
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\server\share\demo")),
        r"\\server\share\demo"
    );
}

#[test]
fn renders_server_home_with_enabled_projects() {
    let projects = vec![
        ProjectConfig {
            name: "mock".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![ApplicationConfig {
                name: "website".to_string(),
                enabled: true,
                target: ApplicationTarget::Static {
                    root: PathBuf::from("."),
                    index_file: "index.html".to_string(),
                    spa_fallback: true,
                },
            }],
        },
        ProjectConfig {
            name: "hidden".to_string(),
            enabled: false,
            project_dir: PathBuf::from("."),
            deployments: Vec::new(),
        },
    ];
    let rendered = render_server_home(
        &BootConfig::default(),
        &projects,
        &HashMap::from([(
            "mock".to_string(),
            GitStatusSummary {
                dirty: true,
                additions: 12,
                deletions: 3,
                ahead: 1,
                behind: 2,
            },
        )]),
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("<title>Latitude Projects - test-host</title>"));
    assert!(rendered.contains("data-latitude-theme-toggle"));
    assert!(rendered.contains("<header><h1>Latitude</h1>"));
    assert!(rendered.contains("Available projects on test-host"));
    assert!(rendered.contains("href=\"/_terminal\""));
    assert!(rendered.contains("Root Terminal"));
    assert!(rendered.contains("Run commands in your user directory"));
    assert!(rendered.contains("href=\"/mock\""));
    assert!(rendered.contains("1 deployment"));
    assert!(rendered.contains("12 additions, 3 deletions, 2 commits to pull, 1 commit to push"));
    assert!(rendered.contains("class=\"git-stat git-additions\">+12"));
    assert!(rendered.contains("class=\"git-stat git-deletions\">-3"));
    assert!(rendered.contains("class=\"git-stat git-behind\" title=\"Commits to pull\">↓2"));
    assert!(rendered.contains("class=\"git-stat git-ahead\" title=\"Commits to push\">↑1"));
    assert!(!rendered.contains("href=\"/hidden\""));
}

#[test]
fn renders_server_home_with_enabled_desktop() {
    let rendered = render_server_home(
        &BootConfig {
            desktop: DesktopConfig {
                enabled: true,
                ..DesktopConfig::default()
            },
            ..BootConfig::default()
        },
        &[],
        &HashMap::new(),
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("href=\"/_desktop\""));
    assert!(rendered.contains("Desktop"));
    assert!(rendered.contains("View the desktop over VNC"));
}

#[test]
fn renders_server_home_with_t3code_link() {
    let rendered = render_server_home(
        &BootConfig {
            t3code: T3CodeConfig {
                enabled: true,
                ..T3CodeConfig::default()
            },
            ..BootConfig::default()
        },
        &[],
        &HashMap::new(),
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("href=\"/__latitude/t3code\" target=\"_blank\" rel=\"noopener\""));
    assert!(rendered.contains("Open T3 Code"));
    assert!(rendered.contains("Open the coding agent workspace"));
}

#[test]
fn builds_public_project_detail_with_enabled_deployments() {
    let detail = public_project_detail(
        &ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![
                ApplicationConfig {
                    name: "website".to_string(),
                    enabled: true,
                    target: ApplicationTarget::ReverseProxy {
                        upstream: "http://127.0.0.1:3000".to_string(),
                        strip_prefix: true,
                    },
                },
                ApplicationConfig {
                    name: "report".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        format: PageFormat::Markdown,
                        media_type: None,
                        title: Some("Weekly Report".to_string()),
                    },
                },
                ApplicationConfig {
                    name: "clip".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        format: PageFormat::Binary,
                        media_type: Some("video/mp4".to_string()),
                        title: Some("Demo Clip".to_string()),
                    },
                },
                ApplicationConfig {
                    name: "recording".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Static {
                        root: PathBuf::from("videos"),
                        index_file: "Screen Recording.mp4".to_string(),
                        spa_fallback: false,
                    },
                },
                ApplicationConfig {
                    name: "draft".to_string(),
                    enabled: false,
                    target: ApplicationTarget::Static {
                        root: PathBuf::from("."),
                        index_file: "index.html".to_string(),
                        spa_fallback: false,
                    },
                },
            ],
        },
        &GitStatusSummary::default(),
        TEST_HOSTNAME,
    );

    assert_eq!(detail.name, "demo");
    assert_eq!(detail.device_hostname, TEST_HOSTNAME);
    assert_eq!(detail.deployment_count, 4);
    assert_eq!(detail.diff.api_href, "/__latitude/api/projects/demo/diff");
    assert_eq!(
        detail.terminal.api_href,
        "/__latitude/api/projects/demo/terminal"
    );
    assert_eq!(detail.deployments[0].kind, "reverse_proxy");
    assert_eq!(detail.deployments[1].kind, "page");
    assert_eq!(
        detail.deployments[1].title.as_deref(),
        Some("Weekly Report")
    );
    assert_eq!(detail.deployments[1].media_type, None);
    assert_eq!(detail.deployments[2].kind, "page");
    assert_eq!(detail.deployments[2].label, "Video document");
    assert_eq!(
        detail.deployments[2].media_type.as_deref(),
        Some("video/mp4")
    );
    assert_eq!(detail.deployments[3].kind, "static");
    assert_eq!(detail.deployments[3].label, "Video document");
    assert_eq!(
        detail.deployments[3].media_type.as_deref(),
        Some("video/mp4")
    );
}

#[test]
fn serves_full_html_document_without_wrapping() {
    let html = "<!doctype html><html><head><title>X</title></head><body>Hi</body></html>";

    assert_eq!(
        render_page_content(None, PageFormat::Html, html, Some("dark"), TEST_HOSTNAME),
        html
    );
}
