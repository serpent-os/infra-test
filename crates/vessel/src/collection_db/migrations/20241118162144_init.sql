-- Add migration script here

CREATE TABLE IF NOT EXISTS collection (
  name TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  package_id TEXT NOT NULL,
  build_release BIGINT NOT NULL,
  source_release BIGINT NOT NULL
);
