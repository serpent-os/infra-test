-- Add migration script here

CREATE TABLE IF NOT EXISTS project (
    project_id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    summary TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS profile (
    profile_id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    arch TEXT NOT NULL,
    index_uri TEXT NOT NULL,
    project_id INT NOT NULL,
    UNIQUE(project_id, name),
    UNIQUE(project_id, index_uri),
    FOREIGN KEY(project_id) REFERENCES project(project_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS profile_remote (
    profile_id INT NOT NULL,
    index_uri TEXT NOT NULL,
    name TEXT NOT NULL,
    priority UNSIGNED BIG INT NOT NULL,
    PRIMARY KEY (profile_id, index_uri),
    FOREIGN KEY(profile_id) REFERENCES profile(profile_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS repository (
    repository_id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    summary TEXT NOT NULL,
    description TEXT,
    commit_ref TEXT,
    origin_uri TEXT NOT NULL,
    status TEXT NOT NULL,
    project_id INT NOT NULL,
    UNIQUE(project_id, name),
    UNIQUE(project_id, origin_uri),
    FOREIGN KEY(project_id) REFERENCES project(project_id) ON DELETE CASCADE
);
