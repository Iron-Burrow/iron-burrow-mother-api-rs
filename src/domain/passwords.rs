//! Deliberately small password handling boundary. Passwords are never
//! persisted or logged outside an Argon2id PHC string.

use std::sync::OnceLock;

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};

const MIN_PASSWORD_CHARS: usize = 12;
const MAX_PASSWORD_CHARS: usize = 128;
const SALT_BYTES: usize = 16;
const MEMORY_COST_KIB: u32 = 19_456;
const TIME_COST: u32 = 2;
const PARALLELISM: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PasswordError {
    Invalid,
    HashingUnavailable,
}

pub(crate) fn validate(password: &str) -> Result<(), PasswordError> {
    let length = password.chars().count();
    if (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&length) {
        Ok(())
    } else {
        Err(PasswordError::Invalid)
    }
}

pub(crate) fn hash(password: &str) -> Result<String, PasswordError> {
    validate(password)?;
    hash_unchecked(password)
}

pub(crate) fn verify(password: &str, encoded: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(encoded) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Performs the same Argon2id work used for an account password when an
/// identity is unknown or cannot authenticate, avoiding an email-existence
/// timing oracle.
pub(crate) fn verify_dummy(password: &str) {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    let encoded = DUMMY_HASH.get_or_init(|| {
        hash_unchecked("Iron Burrow dummy password used only for timing equalization")
            .expect("dummy password hash must be constructible")
    });
    let _ = verify(password, encoded);
}

pub(crate) fn needs_rehash(encoded: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(encoded) else {
        return true;
    };
    parsed.algorithm.as_str() != "argon2id"
        || parsed
            .params
            .get_decimal("m")
            .is_none_or(|value| value < MEMORY_COST_KIB)
        || parsed
            .params
            .get_decimal("t")
            .is_none_or(|value| value < TIME_COST)
        || parsed
            .params
            .get_decimal("p")
            .is_none_or(|value| value < PARALLELISM)
}

fn hash_unchecked(password: &str) -> Result<String, PasswordError> {
    let mut bytes = [0_u8; SALT_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| PasswordError::HashingUnavailable)?;
    let salt = SaltString::encode_b64(&bytes).map_err(|_| PasswordError::HashingUnavailable)?;
    let params = Params::new(MEMORY_COST_KIB, TIME_COST, PARALLELISM, None)
        .map_err(|_| PasswordError::HashingUnavailable)?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password(password.as_bytes(), &salt)
        .map(|value| value.to_string())
        .map_err(|_| PasswordError::HashingUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_bounds_are_explicit() {
        assert_eq!(validate(&"a".repeat(11)), Err(PasswordError::Invalid));
        assert!(validate(&"a".repeat(12)).is_ok());
        assert!(validate(&"a".repeat(128)).is_ok());
        assert_eq!(validate(&"a".repeat(129)), Err(PasswordError::Invalid));
    }

    #[test]
    fn argon2id_hashes_verify_and_are_not_rehashed_at_baseline() {
        let encoded = hash("correct horse battery staple").unwrap();
        assert!(encoded.starts_with("$argon2id$"));
        assert!(verify("correct horse battery staple", &encoded));
        assert!(!verify("incorrect horse battery staple", &encoded));
        assert!(!needs_rehash(&encoded));
    }
}
