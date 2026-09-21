-- No CASCADE: rollback must fail instead of dropping dependent rows silently.
DROP TABLE sessions;
DROP TABLE users;
