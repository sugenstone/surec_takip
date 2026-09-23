-- Reverse 008_sections. No CASCADE: the sections table is dropped
-- explicitly (any future dependent FK must block this revert), grants and
-- catalog rows are removed, and the projects composite-unique FK target
-- added by the up migration is removed last.
DROP TABLE sections;

DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id
  AND p.key IN ('sections:create', 'sections:update', 'sections:archive');

DELETE FROM permissions
WHERE key IN ('sections:create', 'sections:update', 'sections:archive');

ALTER TABLE projects DROP CONSTRAINT projects_tenant_workspace_id_key;
