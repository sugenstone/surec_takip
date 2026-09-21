use crate::config::AuthConfig;
use argon2::{
    Argon2, Params, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString, rand_core::OsRng},
};

// Argon2id with centrally configurable parameters. Hashes are stored as PHC
// strings, which embed the parameters so verification stays self-describing.
pub struct PasswordService {
    argon2: Argon2<'static>,
}

#[derive(Debug)]
pub enum HashError {
    ParameterError,
    HashFailed,
}

impl std::fmt::Display for HashError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            HashError::ParameterError => "argon2 parameters are invalid",
            HashError::HashFailed => "password hashing failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for HashError {}

impl PasswordService {
    pub fn new(config: &AuthConfig) -> Result<Self, HashError> {
        let params = Params::new(
            config.argon2_m_cost,
            config.argon2_t_cost,
            config.argon2_p_cost,
            Some(32),
        )
        .map_err(|_| HashError::ParameterError)?;
        Ok(Self {
            argon2: Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params),
        })
    }

    pub fn hash(&self, password: &str) -> Result<String, HashError> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| HashError::HashFailed)
    }

    // An unparsable stored hash is treated as a non-match: deny access
    // without surfacing storage details to the caller or logs.
    pub fn verify(&self, password: &str, stored_hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return false;
        };
        self.argon2
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> PasswordService {
        PasswordService::new(&AuthConfig::fast_for_tests())
            .unwrap_or_else(|_| panic!("fast test parameters must be valid"))
    }

    #[test]
    fn hash_verifies_its_password_and_rejects_others() {
        let service = service();
        let hash = service
            .hash("correct horse battery staple")
            .unwrap_or_else(|_| panic!("hashing must succeed"));
        assert!(hash.starts_with("$argon2id$"));
        assert!(service.verify("correct horse battery staple", &hash));
        assert!(!service.verify("wrong password", &hash));
    }

    #[test]
    fn hash_never_contains_the_plaintext_password() {
        let service = service();
        let password = "plaintext-secret-value-9f3a";
        let hash = service.hash(password).unwrap_or_else(|_| panic!());
        assert!(!hash.contains(password));
        assert!(!hash.contains("plaintext"));
    }

    #[test]
    fn verify_rejects_malformed_stored_hash_without_panicking() {
        let service = service();
        assert!(!service.verify("password", "not-a-phc-hash"));
        assert!(!service.verify("password", ""));
    }

    #[test]
    fn each_hash_uses_a_fresh_salt() {
        let service = service();
        let first = service.hash("same password").unwrap_or_else(|_| panic!());
        let second = service.hash("same password").unwrap_or_else(|_| panic!());
        assert_ne!(first, second);
    }
}
