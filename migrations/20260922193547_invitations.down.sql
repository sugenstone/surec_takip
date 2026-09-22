-- No CASCADE: rollback must fail instead of dropping dependent rows silently.
DROP TABLE invitations;
DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id AND p.key = 'members:invite';
DELETE FROM permissions WHERE key = 'members:invite';
