CREATE TABLE organizations (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 150),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE users (
    id UUID PRIMARY KEY,
    email_canonical TEXT NOT NULL UNIQUE
        CHECK (email_canonical = lower(btrim(email_canonical)))
        CHECK (char_length(email_canonical) BETWEEN 3 AND 320),
    display_name TEXT NOT NULL CHECK (char_length(btrim(display_name)) BETWEEN 1 AND 150),
    password_hash TEXT NOT NULL CHECK (char_length(password_hash) BETWEEN 20 AND 512),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    platform_admin BOOLEAN NOT NULL DEFAULT FALSE,
    auth_version BIGINT NOT NULL DEFAULT 0 CHECK (auth_version >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX users_status_id_idx ON users (status, id);
