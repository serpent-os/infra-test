-- Add migration script here

CREATE TABLE IF NOT EXISTS account (
    account_id TEXT PRIMARY KEY,  
    type TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    public_key TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS account_token (
    account_id TEXT NOT NULL UNIQUE, 
    encoded TEXT NOT NULL,
    expiration BIGINT NOT NULL,
    FOREIGN KEY(account_id) REFERENCES account(account_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS endpoint (
    endpoint_id TEXT PRIMARY KEY, 
    host_address TEXT NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    account_token TEXT,
    api_token TEXT,
    account_id TEXT NOT NULL UNIQUE, 
    description NOT NULL,

    -- Role must preceed role specific fields
    role TEXT NOT NULL,

    -- Builder fields
    work_status TEXT,

    FOREIGN KEY(account_id) REFERENCES account(account_id) ON DELETE CASCADE
);
