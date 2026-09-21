use sqlx::{ConnectOptions, postgres::PgConnectOptions};
use std::{net::SocketAddr, str::FromStr, time::Duration};

// Never derive Debug: connect options include credentials.
pub struct Config {
    pub bind: SocketAddr,
    pub database: PgConnectOptions,
    pub max_connections: u32,
    pub auth: AuthConfig,
}

// Central credential/session policy. Defaults follow the OWASP Argon2id
// recommendation (19 MiB, t=2, p=1) and a conservative 12h session.
#[derive(Clone)]
pub struct AuthConfig {
    pub argon2_m_cost: u32,
    pub argon2_t_cost: u32,
    pub argon2_p_cost: u32,
    pub session_ttl: Duration,
    pub session_cookie_secure: bool,
}

const ARGON2_M_COST_DEFAULT: u32 = 19_456;
const ARGON2_T_COST_DEFAULT: u32 = 2;
const ARGON2_P_COST_DEFAULT: u32 = 1;
const SESSION_TTL_HOURS_DEFAULT: u64 = 12;

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            argon2_m_cost: ARGON2_M_COST_DEFAULT,
            argon2_t_cost: ARGON2_T_COST_DEFAULT,
            argon2_p_cost: ARGON2_P_COST_DEFAULT,
            session_ttl: Duration::from_secs(SESSION_TTL_HOURS_DEFAULT * 60 * 60),
            session_cookie_secure: true,
        }
    }
}

impl AuthConfig {
    // Fast parameters keep unit/integration tests quick without weakening
    // production defaults; they are rejected outside the documented bounds.
    pub fn fast_for_tests() -> Self {
        Self {
            argon2_m_cost: 8_192,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            session_ttl: Duration::from_secs(60 * 60),
            session_cookie_secure: false,
        }
    }

    fn from_values(get: &impl Fn(&str) -> Option<String>) -> Result<Self, &'static str> {
        let mut config = Self::default();
        if let Some(value) = get("ARGON2_M_COST") {
            config.argon2_m_cost = value
                .parse()
                .map_err(|_| "ARGON2_M_COST must be an integer")?;
        }
        if let Some(value) = get("ARGON2_T_COST") {
            config.argon2_t_cost = value
                .parse()
                .map_err(|_| "ARGON2_T_COST must be an integer")?;
        }
        if let Some(value) = get("ARGON2_P_COST") {
            config.argon2_p_cost = value
                .parse()
                .map_err(|_| "ARGON2_P_COST must be an integer")?;
        }
        if let Some(value) = get("SESSION_TTL_HOURS") {
            let hours = value
                .parse::<u64>()
                .map_err(|_| "SESSION_TTL_HOURS must be an integer")?;
            config.session_ttl = Duration::from_secs(hours * 60 * 60);
        }
        if let Some(value) = get("SESSION_COOKIE_SECURE") {
            config.session_cookie_secure = match value.as_str() {
                "true" => true,
                "false" => false,
                _ => return Err("SESSION_COOKIE_SECURE must be true or false"),
            };
        }
        let bounds = [
            (
                config.argon2_m_cost,
                8_192..=1_048_576,
                "ARGON2_M_COST must be between 8192 and 1048576 KiB",
            ),
            (
                config.argon2_t_cost,
                1..=10,
                "ARGON2_T_COST must be between 1 and 10",
            ),
            (
                config.argon2_p_cost,
                1..=8,
                "ARGON2_P_COST must be between 1 and 8",
            ),
        ];
        for (value, range, message) in bounds {
            if !range.contains(&value) {
                return Err(message);
            }
        }
        if config.session_ttl.as_secs() == 0
            || config.session_ttl > Duration::from_secs(720 * 60 * 60)
        {
            return Err("SESSION_TTL_HOURS must be between 1 and 720");
        }
        Ok(config)
    }
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
        let auth = AuthConfig::from_values(&get)?;
        Ok(Self {
            bind,
            database,
            max_connections,
            auth,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database_env(key: &str) -> Option<String> {
        (key == "DATABASE_URL").then(|| "postgres://localhost/platform_test".into())
    }

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

    #[test]
    fn auth_defaults_apply_without_configuration() {
        let config = Config::from_values(database_env).unwrap_or_else(|message| {
            panic!("{}", message);
        });
        assert_eq!(config.auth.argon2_m_cost, 19_456);
        assert_eq!(config.auth.argon2_t_cost, 2);
        assert_eq!(config.auth.argon2_p_cost, 1);
        assert_eq!(config.auth.session_ttl, Duration::from_secs(12 * 60 * 60));
        assert!(config.auth.session_cookie_secure);
    }

    #[test]
    fn auth_parameters_are_configurable_within_bounds() {
        let config = Config::from_values(|key| match key {
            "ARGON2_M_COST" => Some("8192".into()),
            "ARGON2_T_COST" => Some("1".into()),
            "ARGON2_P_COST" => Some("2".into()),
            "SESSION_TTL_HOURS" => Some("24".into()),
            "SESSION_COOKIE_SECURE" => Some("false".into()),
            _ => database_env(key),
        })
        .unwrap_or_else(|message| panic!("{}", message));
        assert_eq!(config.auth.argon2_m_cost, 8_192);
        assert_eq!(config.auth.session_ttl, Duration::from_secs(24 * 60 * 60));
        assert!(!config.auth.session_cookie_secure);
    }

    #[test]
    fn auth_parameters_out_of_bounds_are_rejected() {
        for key in ["ARGON2_M_COST", "ARGON2_T_COST", "ARGON2_P_COST"] {
            let result = Config::from_values(|k| match k {
                "ARGON2_M_COST" if key == "ARGON2_M_COST" => Some("1".into()),
                "ARGON2_T_COST" if key == "ARGON2_T_COST" => Some("99".into()),
                "ARGON2_P_COST" if key == "ARGON2_P_COST" => Some("99".into()),
                _ => database_env(k),
            });
            assert!(result.is_err(), "{key} must reject out-of-bounds values");
        }
        assert!(
            Config::from_values(|k| match k {
                "SESSION_TTL_HOURS" => Some("0".into()),
                _ => database_env(k),
            })
            .is_err()
        );
        assert!(
            Config::from_values(|k| match k {
                "SESSION_COOKIE_SECURE" => Some("yes".into()),
                _ => database_env(k),
            })
            .is_err()
        );
    }
}
