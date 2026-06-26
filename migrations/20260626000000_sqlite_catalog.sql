CREATE TABLE IF NOT EXISTS migration_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
    name TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL,
    project_dir TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS deployments (
    project_name TEXT NOT NULL,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('reverse_proxy', 'static', 'page')),
    upstream TEXT,
    strip_prefix INTEGER,
    static_root TEXT,
    index_file TEXT,
    spa_fallback INTEGER,
    page_format TEXT,
    media_type TEXT,
    title TEXT,
    content_path TEXT,
    content_hash TEXT,
    content_length INTEGER,
    PRIMARY KEY (project_name, name),
    FOREIGN KEY (project_name) REFERENCES projects(name) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS share_links (
    token TEXT PRIMARY KEY,
    project_name TEXT NOT NULL,
    deployment_name TEXT NOT NULL,
    password TEXT,
    expires_at INTEGER,
    FOREIGN KEY (project_name, deployment_name)
        REFERENCES deployments(project_name, name)
        ON DELETE CASCADE
);
