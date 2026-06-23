use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{HeaderValue, Request, header},
};

use crate::{
    config::{ApplicationConfig, ApplicationTarget, LatitudeConfig, PageFormat, ProjectConfig},
    state::AppState,
};

use super::{
    auth::{clean_next_path, public_password_matches, public_request_is_authenticated},
    constants::{AUTH_COOKIE_NAME, LOGIN_PATH},
    git::{
        GitAction, GitDiffReport, GitFileChange, GitFileDiff, parse_diff_file_sections,
        parse_git_action_form, parse_porcelain_status, parse_public_git_action_payload,
    },
    page::{parse_page_payload, render_page_content},
    paths::{
        ProjectPath, display_path, filtered_cookie_header, join_upstream_url, resolve_project_path,
        sanitized_relative_path, split_project_path,
    },
    public::public_project_detail,
    render::{
        SyntaxLanguage, diff_line_class, render_diff_code_output, render_diff_workspace_fragment,
        render_project_diff, render_project_home, render_project_terminal, render_server_home,
        syntax_language_for_path,
    },
    terminal_api::{PublicTerminalInfoResponse, parse_terminal_command_payload},
};

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

#[test]
fn authenticates_public_requests_with_signed_cookie() {
    let config = LatitudeConfig::default();
    let state = AppState::new(PathBuf::from("latitude.test.json"), config.clone());
    let cookie = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .header(header::COOKIE, format!("{AUTH_COOKIE_NAME}={cookie}"))
        .body(Body::empty())
        .unwrap();

    assert!(public_request_is_authenticated(&state, &config, &req));

    let changed_config = LatitudeConfig {
        public_password: "changed".to_string(),
        ..config
    };
    assert!(!public_request_is_authenticated(
        &state,
        &changed_config,
        &req
    ));
}

#[test]
fn authenticates_public_requests_with_bearer_token() {
    let config = LatitudeConfig::default();
    let state = AppState::new(PathBuf::from("latitude.test.json"), config.clone());
    let token = state.public_auth_cookie_value(&config.public_password);
    let req = Request::builder()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    assert!(public_request_is_authenticated(&state, &config, &req));
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
fn infers_html_for_raw_html_payload() {
    let payload = parse_page_payload(None, b"<section><h1>Hello</h1></section>").unwrap();

    assert_eq!(payload.format, PageFormat::Html);
}

#[test]
fn renders_markdown_as_html_document() {
    let rendered = render_page_content(
        None,
        PageFormat::Markdown,
        "# Agent Report\n\n- Done",
        Some("dark"),
    );

    assert!(rendered.contains("<html lang=\"en\" data-latitude-theme=\"dark\">"));
    assert!(rendered.contains("<title>Agent Report</title>"));
    assert!(rendered.contains("<h1>Agent Report</h1>"));
    assert!(rendered.contains("<li>Done</li>"));
}

#[test]
fn renders_project_home_with_enabled_deployments() {
    let rendered = render_project_home(&ProjectConfig {
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
                    content: "# Report".to_string(),
                    format: PageFormat::Markdown,
                    title: Some("Weekly Report".to_string()),
                },
            },
            ApplicationConfig {
                name: "draft".to_string(),
                enabled: false,
                target: ApplicationTarget::Page {
                    content: "# Draft".to_string(),
                    format: PageFormat::Markdown,
                    title: None,
                },
            },
        ],
    });

    assert!(rendered.contains("href=\"/demo/_diff\""));
    assert!(rendered.contains("Code changes"));
    assert!(rendered.contains("href=\"/demo/_terminal\""));
    assert!(rendered.contains("Run commands in the project directory"));
    assert!(rendered.contains("href=\"/demo/website\""));
    assert!(rendered.contains("Static website"));
    assert!(rendered.contains("href=\"/demo/report\""));
    assert!(rendered.contains("Page: Weekly Report"));
    assert!(!rendered.contains("/demo/draft"));
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
    let rendered = render_project_diff(&project, &report);

    assert!(rendered.contains("<title>demo code changes - Latitude</title>"));
    assert!(rendered.contains("href=\"/demo\""));
    assert!(rendered.contains("<h2>Unstaged files</h2>"));
    assert!(rendered.contains("<h2>Staged files</h2>"));
    assert!(rendered.contains("data-diff-workspace"));
    assert!(rendered.contains("data-action-url=\"/demo/_diff\""));
    assert!(rendered.contains("<details class=\"file-card\" data-file-path=\"src/server.rs\">"));
    assert!(rendered.contains("<strong>Unstaged</strong>"));
    assert!(rendered.contains("data-git-action=\"stage_all\""));
    assert!(rendered.contains("data-git-action=\"stage_file\""));
    assert!(rendered.contains("data-path=\"src/server.rs\""));
    assert!(rendered.contains("data-git-action=\"unstage_file\""));
    assert!(rendered.contains("data-path=\"src/new.rs\""));
    assert!(rendered.contains("data-commit-message"));
    assert!(rendered.contains("Commit staged"));
    assert!(rendered.contains("method: 'PATCH'"));
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
        file_changes: vec![GitFileChange {
            path: "README.md".to_string(),
            original_path: None,
            index_status: '?',
            worktree_status: '?',
            diffs: Vec::new(),
        }],
    };

    let rendered = render_diff_workspace_fragment(&report);

    assert!(rendered.contains("data-action-status hidden"));
    assert!(rendered.contains("<h2>Unstaged files</h2>"));
    assert!(rendered.contains("data-git-action=\"stage_file\""));
    assert!(!rendered.contains("<!doctype html>"));
    assert!(!rendered.contains("<script>"));
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
    let rendered = render_project_terminal(&project, &info, Some("signed-token"));

    assert!(rendered.contains("<title>demo terminal - Latitude</title>"));
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
}

#[test]
fn detects_syntax_language_from_path() {
    assert_eq!(
        syntax_language_for_path("src/main.rs"),
        SyntaxLanguage::Rust
    );
    assert_eq!(
        syntax_language_for_path("sites/app/App.svelte"),
        SyntaxLanguage::JavaScript
    );
    assert_eq!(
        syntax_language_for_path("package.json"),
        SyntaxLanguage::Json
    );
    assert_eq!(
        syntax_language_for_path("latitude.example.json"),
        SyntaxLanguage::Json
    );
    assert_eq!(syntax_language_for_path("README.md"), SyntaxLanguage::Plain);
}

#[test]
fn trims_windows_extended_path_prefix_for_display() {
    assert_eq!(
        display_path(Path::new(r"\\?\C:\work\demo")),
        r"C:\work\demo"
    );
}

#[test]
fn renders_server_home_with_enabled_projects() {
    let rendered = render_server_home(&LatitudeConfig {
        projects: vec![
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
        ],
        ..LatitudeConfig::default()
    });

    assert!(rendered.contains("<title>Latitude Projects</title>"));
    assert!(rendered.contains("href=\"/mock\""));
    assert!(rendered.contains("1 deployment"));
    assert!(!rendered.contains("href=\"/hidden\""));
}

#[test]
fn builds_public_project_detail_with_enabled_deployments() {
    let detail = public_project_detail(&ProjectConfig {
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
                    content: "# Report".to_string(),
                    format: PageFormat::Markdown,
                    title: Some("Weekly Report".to_string()),
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
    });

    assert_eq!(detail.name, "demo");
    assert_eq!(detail.deployment_count, 2);
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
}

#[test]
fn serves_full_html_document_without_wrapping() {
    let html = "<!doctype html><html><head><title>X</title></head><body>Hi</body></html>";

    assert_eq!(
        render_page_content(None, PageFormat::Html, html, Some("dark")),
        html
    );
}
