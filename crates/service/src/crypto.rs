use std::{fmt, path::Path};

use base64::Engine;
use derive_more::{Display, From};
use ed25519_dalek::{
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
    Signature, Signer, SECRET_KEY_LENGTH,
};
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

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self(ed25519_dalek::SigningKey::from_bytes(
            bytes
                .try_into()
                .map_err(|_| Error::InvalidPrivateKeyLength {
                    actual: bytes.len(),
                })?,
        )))
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key())
    }

    pub fn der(&self) -> Result<ed25519_dalek::pkcs8::SecretDocument, Error> {
        self.0.to_pkcs8_der().map_err(Error::EncodeDerPrivateKey)
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.0.sign(message)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self(
            ed25519_dalek::SigningKey::read_pkcs8_pem_file(path)
                .map_err(Error::LoadPemPrivateKey)?,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PublicKey(ed25519_dalek::VerifyingKey);

impl PublicKey {
    pub fn encode(&self) -> EncodedPublicKey {
        EncodedPublicKey(base64::prelude::BASE64_STANDARD_NO_PAD.encode(self.0.as_bytes()))
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), Error> {
        self.0
            .verify_strict(message, signature)
            .map_err(Error::VerifySignature)
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

#[derive(Debug, Clone, PartialEq, Eq, From, Display, Serialize, Deserialize)]
pub struct EncodedPublicKey(String);

impl EncodedPublicKey {
    pub fn decode(key: &str) -> Result<PublicKey, Error> {
        let bytes = base64::prelude::BASE64_STANDARD_NO_PAD
            .decode(key)?
            .try_into()
            .unwrap_or_default();

        Ok(PublicKey(
            ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(Error::DecodePublicKey)?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, From, Display)]
pub struct EncodedSignature(String);

impl EncodedSignature {
    pub fn decode(signature: &str) -> Result<Signature, Error> {
        let bytes = base64::prelude::BASE64_STANDARD_NO_PAD
            .decode(signature)?
            .try_into()
            .unwrap_or([0; 64]);

        Ok(Signature::from_bytes(&bytes))
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("base64 decode")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("decode public key")]
    DecodePublicKey(#[source] ed25519_dalek::SignatureError),
    #[error("encode der public key")]
    EncodeDerPublicKey(#[from] ed25519_dalek::pkcs8::spki::Error),
    #[error("encode der private key")]
    EncodeDerPrivateKey(#[source] ed25519_dalek::pkcs8::Error),
    #[error("signature verification")]
    VerifySignature(#[source] ed25519_dalek::SignatureError),
    #[error("load pem private key")]
    LoadPemPrivateKey(#[source] ed25519_dalek::pkcs8::Error),
    #[error(
        "invalid private key length, expected {} got {actual}",
        SECRET_KEY_LENGTH
    )]
    InvalidPrivateKeyLength { actual: usize },
}
