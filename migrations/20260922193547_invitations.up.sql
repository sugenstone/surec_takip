-- Canonical invitation model (First Agent Mission step 14, ADR 0009).
-- State is derived from stored facts, never a mutable status column:
--   pending  = accepted_at IS NULL AND revoked_at IS NULL AND now() < expires_at
--   accepted = accepted_at IS NOT NULL  (terminal)
--   revoked  = revoked_at IS NOT NULL AND accepted_at IS NULL  (terminal)
--   expired  = pending shape but now() >= expires_at (derived, terminal)
-- Only the raw token holder can accept; the DB stores its SHA-256 digest
-- only (same principle as session tokens).
CREATE TABLE invitations (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  email citext NOT NULL,
  token_hash text NOT NULL,
  invited_by_user_id uuid NOT NULL,
  intended_role_id uuid NULL REFERENCES roles (id),
  expires_at timestamptz NOT NULL,
  accepted_at timestamptz NULL,
  accepted_by_user_id uuid NULL,
  revoked_at timestamptz NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  -- Token digests are unique: a tampered/duplicated token cannot alias to a
  -- second invitation.
  CONSTRAINT invitations_token_hash_key UNIQUE (token_hash),
  -- Intended role must belong to the invitation's tenant (composite FK).
  CONSTRAINT invitations_tenant_role_fk
    FOREIGN KEY (tenant_id, intended_role_id) REFERENCES roles (tenant_id, id)
);

-- One PENDING invitation per (tenant, normalized email): citext equality is
-- case-insensitive; a partial unique index keeps expired/revoked/accepted
-- history out of the way while making concurrent duplicate invites resolve
-- to exactly one live token.
CREATE UNIQUE INDEX invitations_tenant_email_pending_unique
  ON invitations (tenant_id, email)
  WHERE accepted_at IS NULL AND revoked_at IS NULL;

CREATE INDEX invitations_tenant_idx ON invitations (tenant_id);
CREATE INDEX invitations_invited_by_idx ON invitations (invited_by_user_id);

-- ---------------------------------------------------------------------------
-- Permission catalog extension: members:invite (enforced by the invitation
-- endpoints in this same milestone). Granted explicitly to the built-in
-- Owner role for BOTH new and existing organizations — Owner never gains
-- future permissions implicitly (ADR 0008).
-- ---------------------------------------------------------------------------
INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'members:invite', 'Invite new members to the organization');

INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
CROSS JOIN permissions p
WHERE r.is_system AND r.name = 'owner' AND p.key = 'members:invite';
