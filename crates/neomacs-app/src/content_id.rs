//! Validated SHA-256 content identities used by packaged immutable assets.

use std::fmt::{Display, Formatter};

use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContentId(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InvalidContentId;

impl ContentId {
    pub(crate) fn parse(value: &str) -> Result<Self, InvalidContentId> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(InvalidContentId);
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn for_bytes(bytes: &[u8]) -> Self {
        let value = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self(value)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ContentId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
