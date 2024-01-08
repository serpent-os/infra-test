use std::fmt;

use base64::Engine;
use derive_more::{Display, From};
use ed25519_dalek::{pkcs8::EncodePrivateKey, SECRET_KEY_LENGTH};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct KeyPair(ed25519_dalek::SigningKey);

impl KeyPair {
    pub fn generate() -> Self {
        let mut rand = rand::thread_rng();
        let key = ed25519_dalek::SigningKey::generate(&mut rand);
        Self(key)
    }

    pub fn to_bytes(&self) -> [u8; SECRET_KEY_LENGTH] {
        self.0.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; SECRET_KEY_LENGTH]) -> Self {
        Self(ed25519_dalek::SigningKey::from_bytes(bytes))
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key())
    }

    pub fn der(&self) -> Result<ed25519_dalek::pkcs8::SecretDocument, Error> {
        Ok(self.0.to_pkcs8_der()?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PublicKey(ed25519_dalek::VerifyingKey);

impl PublicKey {
    pub fn encode(&self) -> EncodedPublicKey {
        EncodedPublicKey(base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(self.0.as_bytes()))
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0.as_bytes()[..]
    }
}

impl TryFrom<String> for PublicKey {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        EncodedPublicKey::decode(value.as_str())
    }
}

impl From<PublicKey> for String {
    fn from(value: PublicKey) -> Self {
        value.to_string()
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.encode().fmt(f)
    }
}

#[derive(Debug, Clone, From, Display, Serialize, Deserialize)]
pub struct EncodedPublicKey(String);

impl EncodedPublicKey {
    pub fn decode(key: &str) -> Result<PublicKey, Error> {
        let bytes = base64::prelude::BASE64_URL_SAFE_NO_PAD
            .decode(key)?
            .try_into()
            .unwrap_or_default();

        Ok(PublicKey(ed25519_dalek::VerifyingKey::from_bytes(&bytes)?))
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("base64 decode: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("decode public key: {0}")]
    DecodePublicKey(#[from] ed25519_dalek::SignatureError),
    #[error("encode der public key: {0}")]
    EncodeDerPublicKey(#[from] ed25519_dalek::pkcs8::spki::Error),
    #[error("encode der private key: {0}")]
    EncodeDerPrivateKey(#[from] ed25519_dalek::pkcs8::Error),
}
