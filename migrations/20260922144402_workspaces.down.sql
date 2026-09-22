-- No CASCADE: rollback must fail instead of dropping dependent rows silently.
DROP TABLE workspace_memberships;
DROP TABLE workspaces;
