-- Canonical tenant boundary (DATABASE_SCHEMA.md §3). organizations.id IS the
-- tenant id; the table therefore carries no tenant_id column of its own.
CREATE TABLE organizations (
  id uuid PRIMARY KEY,
  name text NOT NULL,
  slug citext NOT NULL,
  status text NOT NULL,
  default_timezone text NOT NULL,
  default_locale text NULL,
  default_currency char(3) NULL,
  settings jsonb NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  CONSTRAINT organizations_slug_key UNIQUE (slug),
  CONSTRAINT organizations_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
  CONSTRAINT organizations_slug_length CHECK (char_length(slug::text) BETWEEN 1 AND 64)
);

-- Hard UNIQUE (tenant_id, user_id) covers soft-deleted rows too: a user has at
-- most ONE membership row per organization ever; rejoining reactivates that
-- row instead of inserting a duplicate. This makes duplicate ACTIVE
-- memberships impossible even under concurrency (ADR 0006).
CREATE TABLE organization_memberships (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  user_id uuid NOT NULL REFERENCES users (id),
  status text NOT NULL,
  joined_at timestamptz NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  CONSTRAINT organization_memberships_tenant_user_key UNIQUE (tenant_id, user_id)
);

-- Hot path: every authenticated request resolves the user's organizations.
CREATE INDEX organization_memberships_user_id_idx ON organization_memberships (user_id);
