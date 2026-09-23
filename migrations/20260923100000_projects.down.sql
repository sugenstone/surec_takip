-- Reverse 007_projects. No CASCADE: the projects table is dropped explicitly;
-- any dependent object (a future FK from sections) must block this revert
-- instead of being silently destroyed. Permission grants and catalog rows
-- introduced by the up migration are removed in reverse order.
DROP TABLE projects;

DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id
  AND p.key IN ('projects:create', 'projects:update', 'projects:archive');

DELETE FROM permissions
WHERE key IN ('projects:create', 'projects:update', 'projects:archive');
