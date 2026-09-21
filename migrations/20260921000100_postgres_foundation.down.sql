-- No CASCADE: rollback must fail instead of deleting dependent business columns.
DROP EXTENSION IF EXISTS citext;
