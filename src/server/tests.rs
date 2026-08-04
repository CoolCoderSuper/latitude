mod shares;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use axum::{
    body::{Body, to_bytes},
    extract::State,
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    response::IntoResponse,
};
use tower::ServiceExt;

use crate::{
    command_protocol::CreateDeploymentShareRequest,
    config::{
        ApplicationConfig, ApplicationTarget, BootConfig, DeploymentShareConfig, DesktopConfig,
        PageFormat, ProjectConfig, T3CodeConfig, decode_page_binary_content,
        encode_page_binary_content,
    },
    desktop::DesktopInfoResponse,
    state::AppState,
    storage::{CatalogStore, WorktreeRecord},
};

use super::{
    assets::{embedded_asset_names, public_asset},
    auth::{
        clean_next_path, open_t3code_embed, public_password_matches,
        public_request_is_authenticated,
    },
    command::{
        T3CodeEmbedSessionRequest, create_deployment_share as public_api_create_share,
        create_project, create_t3code_embed_session,
        delete_deployment_share as public_api_delete_share, get_config, get_project_deployment,
        get_project_page_content, list_deployment_shares as public_api_list_shares,
    },
    command_router,
    constants::{
        AUTH_COOKIE_NAME, LATITUDE_THEME_COOKIE, LOGIN_PATH, PUBLIC_API_SHARES_PATH,
        T3CODE_EMBED_COOKIE,
    },
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
        display_path, filtered_cookie_header, join_upstream_url, resolve_project_path,
        sanitized_relative_path,
    },
    public::{
        ShareUiForm, public_project_detail, public_ui_create_share, public_ui_delete_share,
        public_ui_get_shares,
    },
    public_router,
    render::{
        diff_line_class, highlight_diff_lines, render_diff_code_output, render_diff_file_update,
        render_diff_workspace_fragment, render_project_diff, render_project_files,
        render_project_home, render_project_terminal, render_root_desktop, render_root_terminal,
        render_server_home, render_share_dialog_shell, syntax_name_for_path,
    },
    response::html_response,
    terminal_api::{PublicTerminalInfoResponse, parse_terminal_command_payload},
};

const TEST_HOSTNAME: &str = "test-host";

#[derive(Default)]
struct CatalogFixture {
    share_links: Vec<DeploymentShareConfig>,
    projects: Vec<ProjectFixture>,
}

struct ProjectFixture {
    name: String,
    enabled: bool,
    project_dir: PathBuf,
    deployments: Vec<DeploymentFixture>,
}

struct DeploymentFixture {
    name: String,
    enabled: bool,
    target: DeploymentTargetFixture,
}

enum DeploymentTargetFixture {
    ReverseProxy {
        upstream: String,
        strip_prefix: bool,
    },
    Static {
        root: PathBuf,
        index_file: String,
        spa_fallback: bool,
    },
    Page {
        content: String,
        format: PageFormat,
        media_type: Option<String>,
        title: Option<String>,
    },
}

async fn public_response(state: AppState, request: Request<Body>) -> axum::response::Response {
    public_router(state).oneshot(request).await.unwrap()
}

async fn command_response(state: AppState, request: Request<Body>) -> axum::response::Response {
    command_router(state).oneshot(request).await.unwrap()
}

async fn test_state(config: BootConfig) -> AppState {
    test_state_with_fixture(config, CatalogFixture::default()).await
}

async fn test_state_with_fixture(config: BootConfig, fixture: CatalogFixture) -> AppState {
    let data_dir = std::env::temp_dir().join(format!(
        "latitude-test-data-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let catalog = CatalogStore::open_for_tests(data_dir).await.unwrap();
    install_fixture(&catalog, fixture).await;
    AppState::new(PathBuf::from("latitude.test.json"), config, catalog)
}

async fn install_fixture(catalog: &CatalogStore, fixture: CatalogFixture) {
    for project in fixture.projects {
        let mut deployments = Vec::new();
        let mut pages = Vec::new();
        for deployment in project.deployments {
            match deployment.target {
                DeploymentTargetFixture::ReverseProxy {
                    upstream,
                    strip_prefix,
                } => deployments.push(ApplicationConfig {
                    name: deployment.name,
                    enabled: deployment.enabled,
                    target: ApplicationTarget::ReverseProxy {
                        upstream,
                        strip_prefix,
                    },
                }),
                DeploymentTargetFixture::Static {
                    root,
                    index_file,
                    spa_fallback,
                } => deployments.push(ApplicationConfig {
                    name: deployment.name,
                    enabled: deployment.enabled,
                    target: ApplicationTarget::Static {
                        root,
                        index_file,
                        spa_fallback,
                    },
                }),
                DeploymentTargetFixture::Page {
                    content,
                    format,
                    media_type,
                    title,
                } => pages.push((deployment.name, content, format, media_type, title)),
            }
        }
        let project_name = project.name.clone();
        catalog
            .create_project(ProjectConfig {
                name: project.name,
                enabled: project.enabled,
                project_dir: project.project_dir,
                deployments,
            })
            .await
            .unwrap();
        for (name, content, format, media_type, title) in pages {
            let bytes = if format == PageFormat::Binary {
                decode_page_binary_content(&content).unwrap()
            } else {
                content.into_bytes()
            };
            catalog
                .upsert_page(&project_name, &name, format, media_type, title, bytes)
                .await
                .unwrap();
        }
    }
    for share in fixture.share_links {
        catalog.insert_share_for_tests(&share).await.unwrap();
    }
}

#[tokio::test]
async fn public_pages_render_login_challenge_without_authentication() {
    let response = public_response(
        test_state(BootConfig::default()).await,
        Request::builder().uri("/").body(Body::empty()).unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
}

fn demo_fixture(deployments: Vec<DeploymentFixture>) -> CatalogFixture {
    demo_fixture_with_shares(deployments, Vec::new())
}

fn demo_fixture_with_shares(
    deployments: Vec<DeploymentFixture>,
    share_links: Vec<DeploymentShareConfig>,
) -> CatalogFixture {
    CatalogFixture {
        share_links,
        projects: vec![ProjectFixture {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments,
        }],
    }
}

fn fixture_page(
    name: &str,
    content: &str,
    format: PageFormat,
    media_type: Option<&str>,
    title: Option<&str>,
) -> DeploymentFixture {
    DeploymentFixture {
        name: name.to_string(),
        enabled: true,
        target: DeploymentTargetFixture::Page {
            content: content.to_string(),
            format,
            media_type: media_type.map(str::to_string),
            title: title.map(str::to_string),
        },
    }
}

fn fixture_static(name: &str, root: PathBuf, index_file: &str) -> DeploymentFixture {
    DeploymentFixture {
        name: name.to_string(),
        enabled: true,
        target: DeploymentTargetFixture::Static {
            root,
            index_file: index_file.to_string(),
            spa_fallback: false,
        },
    }
}

fn fixture_reverse_proxy(name: &str, upstream: String) -> DeploymentFixture {
    DeploymentFixture {
        name: name.to_string(),
        enabled: true,
        target: DeploymentTargetFixture::ReverseProxy {
            upstream,
            strip_prefix: true,
        },
    }
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
async fn t3code_embed_session_authenticates_the_browser_and_carries_theme() {
    let config = BootConfig::default();
    let state = test_state_with_fixture(config.clone(), demo_fixture(Vec::new())).await;
    let response = create_t3code_embed_session(
        State(state.clone()),
        axum::Json(T3CodeEmbedSessionRequest {
            project: "demo".to_string(),
            theme: "dark".to_string(),
        }),
    )
    .await
    .unwrap()
    .into_response();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let href = value["href"].as_str().unwrap();
    assert!(href.starts_with("/__latitude/t3code/embed?"));

    let request = Request::builder().uri(href).body(Body::empty()).unwrap();
    let response = open_t3code_embed(State(state), request).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/demo?latitude_t3code_embed=1"
    );
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with(AUTH_COOKIE_NAME))
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with(&format!("{T3CODE_EMBED_COOKIE}=1")))
    );
    let embed_cookie = cookies
        .iter()
        .find(|cookie| cookie.starts_with(&format!("{T3CODE_EMBED_COOKIE}=1")))
        .unwrap();
    assert!(!embed_cookie.contains("Max-Age="));
    assert!(!embed_cookie.contains("Expires="));
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with(&format!("{LATITUDE_THEME_COOKIE}=dark")))
    );
}

#[tokio::test]
async fn t3code_session_cookie_does_not_hide_open_action_from_normal_pages() {
    let config = BootConfig {
        t3code: T3CodeConfig {
            enabled: true,
            ..T3CodeConfig::default()
        },
        ..BootConfig::default()
    };
    let state = test_state_with_fixture(config.clone(), demo_fixture(Vec::new())).await;
    let auth = state.public_auth_cookie_value(&config.public_password);
    let request = Request::builder()
        .uri("/demo")
        .header(
            header::COOKIE,
            format!(
                "{AUTH_COOKIE_NAME}={auth}; {T3CODE_EMBED_COOKIE}=1; {LATITUDE_THEME_COOKIE}=light"
            ),
        )
        .body(Body::empty())
        .unwrap();
    let response = public_response(state, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();
    assert!(rendered.contains("Open in T3 Code"));
    assert!(rendered.contains("data-t3code-open"));
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
        filtered_cookie_header(&value, &[AUTH_COOKIE_NAME]).as_deref(),
        Some("app=one; theme=dark")
    );

    let value = HeaderValue::from_static("latitude_public_session=secret");
    assert_eq!(filtered_cookie_header(&value, &[AUTH_COOKIE_NAME]), None);
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

    let rendered = render_server_home(
        &BootConfig::default(),
        &[],
        &HashMap::new(),
        &[],
        false,
        TEST_HOSTNAME,
    );
    assert!(!rendered.contains("prefers-color-scheme"));
    assert!(!rendered.contains("matchMedia('(prefers-color-scheme"));
    assert!(
        rendered
            .contains("rel=\"icon\" type=\"image/png\" href=\"/__latitude/assets/favicon.png\"")
    );
    assert!(rendered.contains("src=\"/__latitude/assets/theme-bootstrap.js\""));
    assert!(rendered.contains("src=\"/__latitude/assets/theme-toggle.js\""));
    assert!(!rendered.contains("var cookieName"));
}

#[test]
fn project_git_polling_marks_only_periodic_requests_as_auto_refresh() {
    let script = include_str!("assets/project-home.js");

    assert!(script.contains("refreshGitStatuses(false, true)"));
    assert!(script.contains("refreshGitStatuses(true, true)"));
    assert!(script.contains("startVisiblePolling"));
    assert!(!script.contains("setInterval"));
    assert!(script.contains("params.set('refresh', 'auto')"));
}

#[test]
fn browser_tools_use_checked_in_bundles_and_visible_focus() {
    let file_viewer = include_str!("assets/file-viewer.js");
    let diff_viewer = include_str!("assets/diff-viewer.js");
    let terminal_viewer = include_str!("assets/terminal-viewer.js");
    let desktop_style = include_str!("assets/desktop-viewer.css");

    assert!(!file_viewer.contains("https://"));
    assert!(!terminal_viewer.contains("https://"));
    assert!(file_viewer.contains("LatestRequest"));
    assert!(file_viewer.contains("history[replace ? 'replaceState' : 'pushState']"));
    assert!(file_viewer.contains("addEventListener('popstate'"));
    assert!(file_viewer.contains("event.key.toLowerCase() === 'p'"));
    assert!(file_viewer.contains("event.key.toLowerCase() === 'g'"));
    assert!(diff_viewer.contains("event.detail.elt.matches('.commit-form')"));
    assert!(diff_viewer.contains("messageInput.value = ''"));
    assert!(desktop_style.contains(".desktop-canvas:focus-visible"));
}

#[test]
fn generated_html_responses_disable_storage_and_sniffing() {
    let response = html_response(&Method::GET, "<p>Latitude</p>".to_string());

    assert_eq!(
        response.headers().get(header::CACHE_CONTROL),
        Some(&HeaderValue::from_static("no-store"))
    );
    assert_eq!(
        response.headers().get("x-content-type-options"),
        Some(&HeaderValue::from_static("nosniff"))
    );
    assert_eq!(
        response.headers().get("referrer-policy"),
        Some(&HeaderValue::from_static("same-origin"))
    );
}

#[test]
fn t3code_embed_ui_supports_iframes_and_marked_desktop_webviews() {
    let bootstrap = include_str!("assets/theme-bootstrap.js");
    let theme_toggle = include_str!("assets/theme-toggle.js");
    let common_theme = include_str!("assets/common-theme.css");

    assert!(bootstrap.contains("window.self !== window.top"));
    assert!(bootstrap.contains("latitude_t3code_embed_session"));
    assert!(bootstrap.contains("URLSearchParams(window.location.search)"));
    assert!(bootstrap.contains("window.sessionStorage.setItem(marker, '1')"));
    assert!(theme_toggle.contains("dataset.latitudeT3codeEmbed === 'true'"));
    assert!(common_theme.contains("[data-latitude-t3code-embed="));
    assert!(common_theme.contains("[data-t3code-open]"));
}

#[tokio::test]
async fn serves_embedded_assets_with_cache_validation() {
    assert!(embedded_asset_names().any(|name| name == "favicon.png"));
    assert!(embedded_asset_names().any(|name| name == "htmx.min.js"));
    assert!(embedded_asset_names().any(|name| name == "file-viewer.bundle.js"));
    assert!(embedded_asset_names().any(|name| name == "terminal-viewer.bundle.js"));
    assert!(embedded_asset_names().any(|name| name == "terminal-viewer.bundle.css"));
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

    let response = public_asset(
        axum::extract::Path("favicon.png".to_string()),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.starts_with(b"\x89PNG\r\n\x1a\n"));
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

#[tokio::test]
async fn reverse_proxy_streams_upstream_chunks_without_waiting_for_completion() {
    use futures_util::StreamExt;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
        time::{Duration, timeout},
    };

    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release_tail_tx, release_tail_rx) = oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 1024];
        let _ = socket.read(&mut request).await.unwrap();
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\n\
                  content-type: text/plain\r\n\
                  transfer-encoding: chunked\r\n\
                  \r\n\
                  5\r\nfirst\r\n",
            )
            .await
            .unwrap();
        release_tail_rx.await.unwrap();
        socket.write_all(b"6\r\nsecond\r\n0\r\n\r\n").await.unwrap();
    });

    let config = BootConfig::default();
    let state = test_state_with_fixture(
        config.clone(),
        demo_fixture(vec![fixture_reverse_proxy(
            "live",
            format!("http://{address}"),
        )]),
    )
    .await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let request = Request::builder()
        .uri("/demo/live/")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = public_response(state, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body().into_data_stream();
    let first = timeout(Duration::from_secs(1), body.next())
        .await
        .expect("the first chunk should not wait for the complete upstream response")
        .expect("the response should contain a first chunk")
        .expect("the first chunk should be readable");
    assert_eq!(first.as_ref(), b"first");

    release_tail_tx.send(()).unwrap();
    let second = timeout(Duration::from_secs(1), body.next())
        .await
        .expect("the released tail should arrive")
        .expect("the response should contain a second chunk")
        .expect("the second chunk should be readable");
    assert_eq!(second.as_ref(), b"second");
    upstream.await.unwrap();
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
    let state = test_state(BootConfig::default()).await;

    let response = get_config(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(value.get("public_bind").is_some());
    assert!(value.get("projects").is_none());
    assert!(value.get("share_links").is_none());
}

#[tokio::test]
async fn command_project_create_discovers_the_requested_worktree_without_creating_a_duplicate() {
    let repository_dir = std::env::temp_dir().join(format!(
        "latitude-command-project-list-repo-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let worktree_dir = repository_dir.with_extension("worktree");
    std::fs::create_dir_all(&repository_dir).unwrap();
    let git = |directory: &Path, args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(directory)
            .status()
            .expect("git should run");
        assert!(status.success(), "git {args:?} should succeed");
    };
    git(&repository_dir, &["init", "--quiet"]);
    git(&repository_dir, &["config", "user.name", "Latitude Tests"]);
    git(
        &repository_dir,
        &["config", "user.email", "latitude@example.invalid"],
    );
    std::fs::write(repository_dir.join("README.md"), "# Demo\n").unwrap();
    git(&repository_dir, &["add", "README.md"]);
    git(&repository_dir, &["commit", "--quiet", "-m", "initial"]);
    let worktree_arg = worktree_dir.to_string_lossy().into_owned();
    git(
        &repository_dir,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "codex/fix",
            &worktree_arg,
        ],
    );

    let seed = CatalogFixture {
        projects: vec![ProjectFixture {
            name: "demo".to_string(),
            enabled: true,
            project_dir: repository_dir,
            deployments: Vec::new(),
        }],
        ..CatalogFixture::default()
    };
    let state = test_state_with_fixture(BootConfig::default(), seed).await;

    let response = create_project(
        State(state.clone()),
        axum::Json(ProjectConfig {
            name: "demo-2".to_string(),
            enabled: true,
            project_dir: worktree_dir,
            deployments: Vec::new(),
        }),
    )
    .await
    .unwrap()
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let project: ProjectConfig = serde_json::from_slice(&body).unwrap();

    assert_eq!(project.name, "demo--fix");
    assert!(
        state
            .catalog()
            .get_project("demo-2")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn command_deployment_response_omits_page_content_and_content_endpoint_returns_bytes() {
    let seed = CatalogFixture {
        projects: vec![ProjectFixture {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![DeploymentFixture {
                name: "report".to_string(),
                enabled: true,
                target: DeploymentTargetFixture::Page {
                    content: "# Report".to_string(),
                    format: PageFormat::Markdown,
                    media_type: None,
                    title: Some("Report".to_string()),
                },
            }],
        }],
        ..CatalogFixture::default()
    };
    let state = test_state_with_fixture(BootConfig::default(), seed).await;

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
        false,
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
    assert!(rendered.contains("data-t3code-open"));
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
    assert!(rendered.contains("type=\"module\" src=\"/__latitude/assets/project-home.js\""));
    assert!(!rendered.contains("/__latitude/api/shares"));
    assert!(!rendered.contains("/demo/draft"));
    assert!(!rendered.contains("data-deployment=\"draft\""));
    assert!(rendered.contains("View archived (1)"));
    assert!(
        rendered.contains("hx-patch=\"/__latitude/ui/projects/demo/deployments/website/archive\"")
    );
    assert!(!rendered.contains("Archived deployments"));
}

#[test]
fn renders_and_restores_archived_deployments() {
    let rendered = render_project_home(
        &ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![ApplicationConfig {
                name: "draft".to_string(),
                enabled: false,
                target: ApplicationTarget::Page {
                    format: PageFormat::Markdown,
                    media_type: None,
                    title: Some("Draft report".to_string()),
                },
            }],
        },
        &GitStatusSummary::default(),
        false,
        true,
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("Archived deployments"));
    assert!(rendered.contains("Draft report"));
    assert!(rendered.contains("href=\"/demo\">Hide archived"));
    assert!(rendered.contains(
        "hx-patch=\"/__latitude/ui/projects/demo/deployments/draft/archive?archived=false\""
    ));
    assert!(!rendered.contains("href=\"/demo/draft\""));
}

#[tokio::test]
async fn archives_and_restores_deployments_from_project_ui() {
    let config = BootConfig::default();
    let state = test_state_with_fixture(
        config.clone(),
        demo_fixture(vec![fixture_static(
            "website",
            PathBuf::from("."),
            "index.html",
        )]),
    )
    .await;
    let token = state.public_auth_cookie_value(&config.public_password);

    let archive = public_response(
        state.clone(),
        Request::builder()
            .method(Method::PATCH)
            .uri("/__latitude/ui/projects/demo/deployments/website/archive")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(archive.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        archive.headers().get("HX-Trigger").unwrap(),
        "deploymentArchived"
    );
    assert!(
        !state
            .catalog()
            .get_deployment("demo", "website")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    let unavailable = public_response(
        state.clone(),
        Request::builder()
            .uri("/demo/website")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unavailable.status(), StatusCode::NOT_FOUND);

    let restore = public_response(
        state.clone(),
        Request::builder()
            .method(Method::PATCH)
            .uri("/__latitude/ui/projects/demo/deployments/website/archive?archived=false")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::NO_CONTENT);
    assert!(
        state
            .catalog()
            .get_deployment("demo", "website")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
}

#[tokio::test]
async fn command_api_archives_and_restores_deployments() {
    let state = test_state_with_fixture(
        BootConfig::default(),
        demo_fixture(vec![fixture_static(
            "website",
            PathBuf::from("."),
            "index.html",
        )]),
    )
    .await;

    let archive = command_response(
        state.clone(),
        Request::builder()
            .method(Method::PATCH)
            .uri("/api/projects/demo/deployments/website/archive")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"archived":true}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(archive.status(), StatusCode::OK);
    let body = to_bytes(archive.into_body(), usize::MAX).await.unwrap();
    let deployment: ApplicationConfig = serde_json::from_slice(&body).unwrap();
    assert!(!deployment.enabled);

    let restore = command_response(
        state,
        Request::builder()
            .method(Method::PATCH)
            .uri("/api/projects/demo/deployments/website/archive")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"archived":false}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::OK);
    let body = to_bytes(restore.into_body(), usize::MAX).await.unwrap();
    let deployment: ApplicationConfig = serde_json::from_slice(&body).unwrap();
    assert!(deployment.enabled);
}

#[tokio::test]
async fn serves_binary_page_document_shell_by_default() {
    let config = BootConfig::default();
    let seed = demo_fixture(vec![fixture_page(
        "snapshot",
        &encode_page_binary_content(b"png bytes"),
        PageFormat::Binary,
        Some("image/png"),
        None,
    )]);
    let state = test_state_with_fixture(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
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
    let seed = demo_fixture(vec![fixture_page(
        "snapshot",
        &encode_page_binary_content(b"png bytes"),
        PageFormat::Binary,
        Some("image/png"),
        Some("Build Snapshot"),
    )]);
    let state = test_state_with_fixture(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "image/avif,image/webp,image/png,*/*;q=0.8")
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
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
    let seed = demo_fixture(vec![fixture_page(
        "snapshot",
        &encode_page_binary_content(b"png bytes"),
        PageFormat::Binary,
        Some("image/png"),
        Some("Build Snapshot"),
    )]);
    let state = test_state_with_fixture(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot?raw=1")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "text/html")
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
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
    let seed = demo_fixture(vec![fixture_static(
        "snapshot",
        root.clone(),
        "snapshot.png",
    )]);
    let state = test_state_with_fixture(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
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
    let seed = demo_fixture(vec![fixture_static(
        "snapshot",
        root.clone(),
        "snapshot.png",
    )]);
    let state = test_state_with_fixture(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/snapshot?raw=1")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
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
    let seed = demo_fixture(vec![fixture_static("website", root.clone(), "index.html")]);
    let state = test_state_with_fixture(config.clone(), seed).await;
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .uri("/demo/website")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = public_response(state, req).await;
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
        error: None,
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
    assert!(rendered.contains("data-stage-action"));
    assert!(rendered.contains("data-unstage-action"));
    assert!(rendered.contains("data-file-select"));
    assert!(rendered.contains("form=\"stage-action-form\""));
    assert!(rendered.contains("form=\"unstage-action-form\""));
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
    assert!(rendered.contains("type=\"module\" src=\"/__latitude/assets/diff-viewer.js\""));
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
        error: None,
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
fn renders_git_collection_error_in_workspace() {
    let report = GitDiffReport {
        repo_dir: PathBuf::from("C:/work/demo"),
        status: GitStatusSummary::default(),
        error: Some("git status failed <unexpectedly>".to_string()),
        file_changes: Vec::new(),
    };

    let rendered = render_diff_workspace_fragment(&report, "/demo/_diff").into_string();

    assert!(rendered.contains("data-git-collection-error"));
    assert!(rendered.contains("git status failed &lt;unexpectedly&gt;"));
}

#[test]
fn renders_targeted_diff_file_update() {
    let report = GitDiffReport {
        repo_dir: PathBuf::from("C:/work/demo"),
        status: GitStatusSummary::default(),
        error: None,
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
        parse_git_action_form(b"action=stage_selected&path=src%2Fserver.rs&path=README.md")
            .unwrap(),
        GitAction::StageFiles {
            paths: vec!["src/server.rs".to_string(), "README.md".to_string()]
        }
    );
    assert!(parse_git_action_form(b"action=stage_selected").is_err());
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
        parse_git_action_form(b"action=unstage_selected&path=src%2Fserver.rs&path=README.md")
            .unwrap(),
        GitAction::UnstageFiles {
            paths: vec!["src/server.rs".to_string(), "README.md".to_string()]
        }
    );
    assert!(parse_git_action_form(b"action=unstage_selected").is_err());
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
            br#"{"action":"stage_selected","paths":["src\\server.rs","README.md"]}"#,
        )
        .unwrap(),
        GitAction::StageFiles {
            paths: vec!["src/server.rs".to_string(), "README.md".to_string()]
        }
    );
    assert_eq!(
        parse_public_git_action_payload(
            Some("application/json"),
            br#"{"action":"unstage_selected","paths":["src\\server.rs","README.md"]}"#,
        )
        .unwrap(),
        GitAction::UnstageFiles {
            paths: vec!["src/server.rs".to_string(), "README.md".to_string()]
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
    assert!(rendered.contains("type=\"module\" src=\"/__latitude/assets/file-viewer.bundle.js\""));
    assert!(rendered.contains("data-find-file"));
    assert!(rendered.contains("data-grep-search"));
    assert!(rendered.contains("data-search-palette"));
    assert!(rendered.contains("data-search-preview-content"));
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
    let seed = CatalogFixture {
        share_links: Vec::new(),
        projects: vec![ProjectFixture {
            name: "demo".to_string(),
            enabled: true,
            project_dir,
            deployments: Vec::new(),
        }],
    };
    let config = BootConfig::default();
    let state = test_state_with_fixture(config.clone(), seed).await;
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
    assert!(rendered.contains("href=\"/__latitude/assets/terminal-viewer.bundle.css\""));
    assert!(
        rendered.contains("type=\"module\" src=\"/__latitude/assets/terminal-viewer.bundle.js\"")
    );
    assert!(!rendered.contains("cdn.jsdelivr.net"));
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
    assert!(rendered.contains("href=\"/__latitude/assets/desktop-viewer.css\""));
    assert!(rendered.contains("src=\"/__latitude/assets/desktop-viewer.js\""));
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

    let response = public_response(state, req).await;
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

    let response = public_response(state, req).await;
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

    let response = public_response(state, req).await;
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
        error: None,
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
    assert!(payload["error"].is_null());
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
        &[],
        false,
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
fn groups_linked_worktrees_on_server_home() {
    let projects = vec![
        ProjectConfig {
            name: "latitude".to_string(),
            enabled: true,
            project_dir: PathBuf::from("C:/work/latitude"),
            deployments: Vec::new(),
        },
        ProjectConfig {
            name: "latitude--mobile-fix".to_string(),
            enabled: true,
            project_dir: PathBuf::from("C:/work/latitude-mobile-fix"),
            deployments: Vec::new(),
        },
    ];
    let common_git_dir = PathBuf::from("C:/work/latitude/.git");
    let worktrees = vec![
        WorktreeRecord {
            project_name: "latitude".to_string(),
            common_git_dir: common_git_dir.clone(),
            worktree_dir: projects[0].project_dir.clone(),
            branch: Some("master".to_string()),
            head: "abc123".to_string(),
            discovered: false,
            archived: false,
        },
        WorktreeRecord {
            project_name: "latitude--mobile-fix".to_string(),
            common_git_dir,
            worktree_dir: PathBuf::from(r"\\?\C:\work\latitude-mobile-fix"),
            branch: Some("codex/mobile-fix".to_string()),
            head: "def456".to_string(),
            discovered: true,
            archived: false,
        },
    ];

    let rendered = render_server_home(
        &BootConfig::default(),
        &projects,
        &HashMap::new(),
        &worktrees,
        false,
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("class=\"worktree-group\""));
    assert!(rendered.contains("<strong>latitude</strong>"));
    assert!(rendered.contains("2 worktrees"));
    assert!(rendered.contains("href=\"/latitude--mobile-fix\""));
    assert!(rendered.contains("codex/mobile-fix"));
    assert!(rendered.contains(r"C:\work\latitude-mobile-fix"));
    assert!(!rendered.contains(r"\\?\C:\work\latitude-mobile-fix"));
    assert!(rendered.contains("data-project-list"));
    assert!(rendered.contains("hx-get=\"/?refresh=auto\""));
    assert!(rendered.contains(
        "hx-trigger=\"every 5s [document.visibilityState === 'visible'], worktreeArchived from:body\""
    ));
    assert!(rendered.contains("hx-target=\"#project-list\""));
    assert!(rendered.contains("hx-sync=\"this:drop\""));
    assert!(rendered.contains("id=\"project-git-status-latitude--mobile-fix\""));
    assert!(rendered.contains("hx-preserve"));
    assert!(!rendered.contains("hx-patch=\"/__latitude/ui/projects/latitude/archive\""));
    assert!(rendered.contains("hx-patch=\"/__latitude/ui/projects/latitude--mobile-fix/archive\""));
    assert!(rendered.contains("hx-swap=\"none\""));
    assert!(rendered.contains("aria-label=\"Archive codex/mobile-fix\""));
}

#[test]
fn renders_and_restores_archived_projects_on_server_home() {
    let projects = vec![
        ProjectConfig {
            name: "latitude".to_string(),
            enabled: true,
            project_dir: PathBuf::from("C:/work/latitude"),
            deployments: Vec::new(),
        },
        ProjectConfig {
            name: "latitude--finished".to_string(),
            enabled: true,
            project_dir: PathBuf::from("C:/work/latitude-finished"),
            deployments: Vec::new(),
        },
    ];
    let common_git_dir = PathBuf::from("C:/work/latitude/.git");
    let worktrees = vec![
        WorktreeRecord {
            project_name: "latitude".to_string(),
            common_git_dir: common_git_dir.clone(),
            worktree_dir: projects[0].project_dir.clone(),
            branch: Some("master".to_string()),
            head: "abc123".to_string(),
            discovered: false,
            archived: false,
        },
        WorktreeRecord {
            project_name: "latitude--finished".to_string(),
            common_git_dir,
            worktree_dir: projects[1].project_dir.clone(),
            branch: Some("codex/finished".to_string()),
            head: "def456".to_string(),
            discovered: true,
            archived: true,
        },
    ];

    let hidden = render_server_home(
        &BootConfig::default(),
        &projects,
        &HashMap::new(),
        &worktrees,
        false,
        TEST_HOSTNAME,
    );
    assert!(hidden.contains("View archived (1)"));
    assert!(hidden.contains("href=\"/?archived=1\""));
    assert!(!hidden.contains("Archived projects"));
    assert!(!hidden.contains("href=\"/latitude--finished\""));

    let shown = render_server_home(
        &BootConfig::default(),
        &projects,
        &HashMap::new(),
        &worktrees,
        true,
        TEST_HOSTNAME,
    );
    assert!(shown.contains("Archived projects"));
    assert!(shown.contains("href=\"/latitude--finished\""));
    assert!(shown.contains("codex/finished"));
    assert!(shown.contains("hx-get=\"/?refresh=auto&amp;archived=1\""));
    assert!(shown.contains(
        "hx-patch=\"/__latitude/ui/projects/latitude--finished/archive?archived=false\""
    ));
    assert!(shown.contains("aria-label=\"Restore codex/finished\""));
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
        &[],
        false,
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("href=\"/_desktop\""));
    assert!(rendered.contains("Desktop"));
    assert!(rendered.contains("View and control the desktop"));
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
        &[],
        false,
        TEST_HOSTNAME,
    );

    assert!(rendered.contains("href=\"/__latitude/t3code\" target=\"_blank\" rel=\"noopener\""));
    assert!(rendered.contains("Open T3 Code"));
    assert!(rendered.contains("Open the coding agent workspace"));
    assert!(rendered.contains("data-t3code-open"));
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
    assert_eq!(detail.deployments.len(), 4);
    assert_eq!(detail.archived_deployments.len(), 1);
    assert_eq!(detail.archived_deployments[0].name, "draft");
    assert_eq!(detail.archived_deployments[0].kind, "static");
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

#[tokio::test]
async fn coalesces_concurrent_git_refresh_requests() {
    let state = test_state(BootConfig::default()).await;
    let crate::state::GitRefreshAccess::Leader(leader) = state
        .acquire_git_refresh(false, std::time::Duration::from_millis(500))
        .await
    else {
        panic!("the first request should lead the refresh");
    };
    let waiting_state = state.clone();
    let waiter = tokio::spawn(async move {
        waiting_state
            .acquire_git_refresh(false, std::time::Duration::from_millis(500))
            .await
    });
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());

    leader.complete();
    assert!(matches!(
        waiter.await.unwrap(),
        crate::state::GitRefreshAccess::Reused
    ));
}

#[tokio::test]
async fn remote_fetch_request_upgrades_after_local_refresh() {
    let state = test_state(BootConfig::default()).await;
    let crate::state::GitRefreshAccess::Leader(local_refresh) = state
        .acquire_git_refresh(false, std::time::Duration::from_millis(500))
        .await
    else {
        panic!("the first request should lead the refresh");
    };
    let waiting_state = state.clone();
    let remote_waiter = tokio::spawn(async move {
        waiting_state
            .acquire_git_refresh(true, std::time::Duration::from_millis(500))
            .await
    });
    tokio::task::yield_now().await;

    local_refresh.complete();
    let crate::state::GitRefreshAccess::Leader(remote_refresh) = remote_waiter.await.unwrap()
    else {
        panic!("a local refresh must not satisfy a remote-fetch request");
    };
    remote_refresh.complete();
}

#[tokio::test]
async fn auto_refresh_reuses_snapshot_until_its_max_age_expires() {
    let state = test_state(BootConfig::default()).await;
    let crate::state::GitRefreshAccess::Leader(initial_refresh) = state
        .acquire_git_refresh(false, std::time::Duration::from_millis(500))
        .await
    else {
        panic!("the initial request should lead the refresh");
    };
    initial_refresh.complete();

    assert!(matches!(
        state
            .acquire_git_refresh(false, std::time::Duration::from_secs(10))
            .await,
        crate::state::GitRefreshAccess::Reused
    ));

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let crate::state::GitRefreshAccess::Leader(expired_refresh) = state
        .acquire_git_refresh(false, std::time::Duration::from_millis(1))
        .await
    else {
        panic!("an expired snapshot should be refreshed");
    };
    expired_refresh.complete();
}
