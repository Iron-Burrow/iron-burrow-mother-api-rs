use sha2::{Digest, Sha256};

const KEY_PREFIX_MARKER: &str = "ib_live_";
const RANDOM_PREFIX_BYTES: usize = 8;
const RANDOM_SECRET_BYTES: usize = 32;
const RANDOM_PREFIX_HEX_LEN: usize = RANDOM_PREFIX_BYTES * 2;
const RANDOM_SECRET_HEX_LEN: usize = RANDOM_SECRET_BYTES * 2;
const SHA256_DIGEST_BYTES: usize = 32;

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct RawApiKey {
    value: String,
}

impl RawApiKey {
    pub(crate) fn generate() -> Result<Self, ApiKeyGenerationError> {
        let mut prefix = [0_u8; RANDOM_PREFIX_BYTES];
        let mut secret = [0_u8; RANDOM_SECRET_BYTES];
        getrandom::fill(&mut prefix).map_err(ApiKeyGenerationError::random)?;
        getrandom::fill(&mut secret).map_err(ApiKeyGenerationError::random)?;

        Ok(Self {
            value: format!(
                "{KEY_PREFIX_MARKER}{}.{}",
                hex::encode(prefix),
                hex::encode(secret)
            ),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_test_value(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub(crate) fn expose_secret(&self) -> &str {
        &self.value
    }

    pub(crate) fn parse(&self) -> Result<ParsedApiKey, ApiKeyFormatError> {
        parse_presented_api_key(&self.value)
    }

    pub(crate) fn key_prefix(&self) -> Result<String, ApiKeyFormatError> {
        Ok(self.parse()?.key_prefix)
    }

    pub(crate) fn sha256_hash(&self) -> [u8; SHA256_DIGEST_BYTES] {
        hash_presented_api_key(&self.value)
    }
}

impl std::fmt::Debug for RawApiKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RawApiKey(<redacted>)")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedApiKey {
    pub(crate) key_prefix: String,
}

pub(crate) fn parse_presented_api_key(value: &str) -> Result<ParsedApiKey, ApiKeyFormatError> {
    let (key_prefix, secret) = value
        .split_once('.')
        .ok_or(ApiKeyFormatError::MissingSeparator)?;

    if secret.contains('.') {
        return Err(ApiKeyFormatError::MultipleSeparators);
    }
    if !key_prefix.starts_with(KEY_PREFIX_MARKER) {
        return Err(ApiKeyFormatError::UnsupportedPrefix);
    }

    let random_prefix = &key_prefix[KEY_PREFIX_MARKER.len()..];
    if random_prefix.len() != RANDOM_PREFIX_HEX_LEN || !is_lower_hex(random_prefix) {
        return Err(ApiKeyFormatError::InvalidRandomPrefix);
    }
    if secret.len() != RANDOM_SECRET_HEX_LEN || !is_lower_hex(secret) {
        return Err(ApiKeyFormatError::InvalidSecret);
    }

    Ok(ParsedApiKey {
        key_prefix: key_prefix.to_string(),
    })
}

pub(crate) fn hash_presented_api_key(value: &str) -> [u8; SHA256_DIGEST_BYTES] {
    Sha256::digest(value.as_bytes()).into()
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("failed to generate API key: {message}")]
pub(crate) struct ApiKeyGenerationError {
    message: String,
}

impl ApiKeyGenerationError {
    fn random(error: getrandom::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ApiKeyFormatError {
    #[error("API key must contain one prefix/secret separator")]
    MissingSeparator,
    #[error("API key must contain only one prefix/secret separator")]
    MultipleSeparators,
    #[error("API key prefix must start with ib_live_")]
    UnsupportedPrefix,
    #[error("API key random prefix must be 16 lowercase hex characters")]
    InvalidRandomPrefix,
    #[error("API key secret must be 64 lowercase hex characters")]
    InvalidSecret,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_has_expected_shape_prefix_and_hash() {
        let key = RawApiKey::generate().unwrap();
        let parsed = key.parse().unwrap();
        let hash = key.sha256_hash();

        assert!(key.expose_secret().starts_with("ib_live_"));
        assert_eq!(parsed.key_prefix.len(), KEY_PREFIX_MARKER.len() + 16);
        assert_eq!(hash.len(), 32);
        assert!(key.expose_secret().starts_with(&parsed.key_prefix));
    }

    #[test]
    fn parses_strict_presented_key_format() {
        let key = "ib_live_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        assert_eq!(
            parse_presented_api_key(key).unwrap(),
            ParsedApiKey {
                key_prefix: "ib_live_0123456789abcdef".to_string()
            }
        );

        for malformed in [
            "ib_live_0123456789abcdef",
            "ib_live_0123456789abcdef.0123.extra",
            "wrong_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "ib_live_0123456789abcdeg.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "ib_live_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg",
            "ib_live_0123456789abcdef.0123",
        ] {
            assert!(
                parse_presented_api_key(malformed).is_err(),
                "malformed key should be rejected: {malformed}"
            );
        }
    }

    #[test]
    fn hashing_is_stable_sha256_of_full_presented_key() {
        let key = "ib_live_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        assert_eq!(
            hex::encode(hash_presented_api_key(key)),
            "5afe70519d6c885e69e2f310e7b55a704d710c541c0991fd2ac32d990145607b"
        );
    }

    #[test]
    fn raw_key_debug_is_redacted() {
        let key = RawApiKey::from_test_value(
            "ib_live_0123456789abcdef.0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );

        assert_eq!(format!("{key:?}"), "RawApiKey(<redacted>)");
    }
}
