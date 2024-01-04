use base64::Engine;
use derive_more::From;
use ed25519_dalek::{pkcs8::EncodePrivateKey, PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};
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

#[derive(Debug, Clone, Copy)]
pub struct PublicKey(ed25519_dalek::VerifyingKey);

impl PublicKey {
    pub fn to_bytes(self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.0.to_bytes()
    }

    pub fn encode(&self) -> EncodedPublicKey {
        EncodedPublicKey(base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(self.0.as_bytes()))
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0.as_bytes()[..]
    }
}

#[derive(Debug, Clone, From)]
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

impl ToString for EncodedPublicKey {
    fn to_string(&self) -> String {
        self.0.clone()
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
