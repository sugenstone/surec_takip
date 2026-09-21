use sqlx::{ConnectOptions, postgres::PgConnectOptions};
use std::{net::SocketAddr, str::FromStr};

// Never derive Debug: connect options include credentials.
pub struct Config {
    pub bind: SocketAddr,
    pub database: PgConnectOptions,
    pub max_connections: u32,
}

impl Config {
    pub fn from_env() -> Result<Self, &'static str> {
        Self::from_values(|key| std::env::var(key).ok())
    }

    pub fn from_values(get: impl Fn(&str) -> Option<String>) -> Result<Self, &'static str> {
        let bind = get("SERVER_BIND")
            .unwrap_or_else(|| "127.0.0.1:8080".into())
            .parse()
            .map_err(|_| "SERVER_BIND must be an IP address and port")?;
        let database_url = get("DATABASE_URL").ok_or("DATABASE_URL is required")?;
        if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
            return Err("DATABASE_URL must be a PostgreSQL URL");
        }
        let database = PgConnectOptions::from_str(&database_url)
            .map_err(|_| "DATABASE_URL is invalid")?
            .disable_statement_logging();
        let max_connections = get("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|| "10".into())
            .parse::<u32>()
            .map_err(|_| "DATABASE_MAX_CONNECTIONS must be an integer between 1 and 100")?;
        if !(1..=100).contains(&max_connections) {
            return Err("DATABASE_MAX_CONNECTIONS must be between 1 and 100");
        }
        Ok(Self {
            bind,
            database,
            max_connections,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_invalid_config_never_exposes_input() {
        assert!(matches!(
            Config::from_values(|_| None),
            Err("DATABASE_URL is required")
        ));
        let result =
            Config::from_values(|key| (key == "DATABASE_URL").then(|| "secret-invalid-url".into()));
        assert!(matches!(
            result,
            Err("DATABASE_URL must be a PostgreSQL URL")
        ));
    }

    #[test]
    fn pool_limits_are_bounded() {
        for limit in ["0", "101", "bad"] {
            assert!(
                Config::from_values(|key| match key {
                    "DATABASE_URL" => Some("postgres://localhost/platform_test".into()),
                    "DATABASE_MAX_CONNECTIONS" => Some(limit.into()),
                    _ => None,
                })
                .is_err()
            );
        }
    }
}
