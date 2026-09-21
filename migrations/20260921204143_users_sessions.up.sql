-- Canonical identity schema (DATABASE_SCHEMA.md §3).
-- citext was installed by 20260921000100_postgres_foundation.
CREATE TABLE users (
  id uuid PRIMARY KEY,
  email citext NOT NULL,
  password_hash text NULL,
  display_name text NOT NULL,
  email_verified_at timestamptz NULL,
  status text NOT NULL,
  locale text NULL,
  timezone text NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT users_email_key UNIQUE (email)
);

CREATE TABLE sessions (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users (id),
  token_hash text NOT NULL,
  expires_at timestamptz NOT NULL,
  revoked_at timestamptz NULL,
  ip_metadata jsonb NULL,
  user_agent text NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT sessions_token_hash_key UNIQUE (token_hash),
  CONSTRAINT sessions_expires_after_created CHECK (expires_at > created_at)
);

-- Hot path: session lookup by token digest on every authenticated request;
-- the UNIQUE constraint above provides the backing index.
-- Revocation-by-user and later cleanup scans.
CREATE INDEX sessions_user_id_idx ON sessions (user_id);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);
