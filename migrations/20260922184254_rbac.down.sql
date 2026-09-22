-- No CASCADE: rollback must fail instead of dropping dependent rows silently.
DROP TABLE membership_roles;
DROP TABLE role_permissions;
DROP TABLE roles;
DROP TABLE permissions;
