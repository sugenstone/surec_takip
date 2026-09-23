-- Dependencies must block rollback instead of being removed implicitly.
DROP TABLE work_items;

DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id
  AND p.key IN ('work_items:create', 'work_items:update', 'work_items:archive');

DELETE FROM permissions
WHERE key IN ('work_items:create', 'work_items:update', 'work_items:archive');
