-- No CASCADE: rollback must fail instead of dropping dependent rows silently.
DROP TABLE organization_memberships;
DROP TABLE organizations;
