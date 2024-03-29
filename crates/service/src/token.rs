use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    account,
    crypto::{self, KeyPair, PublicKey},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    header: Header,
    pub payload: Payload,
}

impl Token {
    pub fn new(payload: Payload) -> Self {
        Self {
            header: Header::new(jsonwebtoken::Algorithm::EdDSA),
            payload,
        }
    }

    /// Verify and return a decoded token
    pub fn verify(
        token: &str,
        public_key: &PublicKey,
        validation: &Validation,
    ) -> Result<VerifiedToken, Error> {
        let decoded = jsonwebtoken::decode::<Payload>(
            token,
            // This actually takes the compressed bytes and not
            // the der encoded pkcs#8 format bytes, such as
            // on the sign / encoding side. Fails otherwise.
            &DecodingKey::from_ed_der(public_key.as_ref()),
            &validation.0,
        )
        .map_err(Error::DecodeToken)?;

        Ok(VerifiedToken {
            encoded: token.to_string(),
            decoded: Token {
                header: decoded.header,
                payload: decoded.claims,
            },
        })
    }

    /// Sign and return an encoded token
    pub fn sign(&self, key_pair: &KeyPair) -> Result<String, Error> {
        jsonwebtoken::encode(
            &self.header,
            &self.payload,
            &EncodingKey::from_ed_der(key_pair.der()?.as_bytes()),
        )
        .map_err(Error::SignToken)
    }

    pub fn is_expired(&self) -> bool {
        let start = std::time::SystemTime::now();
        let now = start
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        self.payload.exp as u64 <= now
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedToken {
    pub encoded: String,
    pub decoded: Token,
}

impl VerifiedToken {
    pub fn expires(&self) -> DateTime<Utc> {
        chrono::NaiveDateTime::from_timestamp_opt(self.decoded.payload.exp, 0)
            .map(|dt| dt.and_utc())
            .unwrap_or_else(|| NaiveDateTime::UNIX_EPOCH.and_utc())
    }
}

#[derive(Debug, Clone)]
pub struct Validation(jsonwebtoken::Validation);

impl Default for Validation {
    fn default() -> Self {
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
        // Expiration is evaluated in the authentication layer
        validation.validate_exp = false;
        validation.validate_aud = false;
        validation.required_spec_claims = ["aud", "exp", "iss", "sub"]
            .into_iter()
            .map(String::from)
            .collect();

        Self(validation)
    }
}

impl Validation {
    /// Create a default validation that verifies signature
    pub fn new() -> Self {
        Self::default()
    }

    /// Validation will check that the `aud` field is is equal to
    /// the provided value
    pub fn aud(mut self, aud: impl ToString) -> Self {
        self.0.validate_aud = true;
        self.0.aud = Some([aud.to_string()].into_iter().collect());
        self
    }

    /// Validation will check that the `iss` field is is equal to
    /// the provided value
    pub fn iss(mut self, iss: impl ToString) -> Self {
        self.0.iss = Some([iss.to_string()].into_iter().collect());
        self
    }

    /// Validation will check that the `sub` field is is equal to
    /// the provided value
    #[allow(clippy::should_implement_trait)]
    pub fn sub(mut self, sub: impl ToString) -> Self {
        self.0.sub = Some(sub.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    // Standard
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    pub sub: String,
    // Internal
    #[serde(rename = "pur")]
    pub purpose: Purpose,
    #[serde(rename = "uid")]
    pub account_id: account::Id,
    #[serde(rename = "act")]
    pub account_type: account::Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Purpose {
    Authorization,
    Authentication,
}

impl Purpose {
    pub fn duration(&self) -> Duration {
        match self {
            Purpose::Authorization => Duration::days(7),
            Purpose::Authentication => Duration::hours(1),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("decode token")]
    DecodeToken(#[source] jsonwebtoken::errors::Error),
    #[error("sign token")]
    SignToken(#[source] jsonwebtoken::errors::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::Error),
}

#[cfg(test)]
mod test {
    // use base64::Engine;
    use chrono::{Duration, Utc};
    use jsonwebtoken::Algorithm;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn roundtrip() {
        let keypair = KeyPair::generate();

        let now = Utc::now();
        let one_hour = now + Duration::seconds(60 * 60);

        let token = Token {
            header: Header::new(Algorithm::EdDSA),
            payload: Payload {
                aud: "test".into(),
                exp: one_hour.timestamp(),
                iat: now.timestamp(),
                iss: "test".into(),
                sub: "test".into(),
                purpose: Purpose::Authorization,
                account_id: Uuid::new_v4().into(),
                account_type: account::Kind::Admin,
            },
        };

        // Round trip
        let encoded = token.sign(&keypair).unwrap();
        let verified = Token::verify(&encoded, &keypair.public_key(), &Validation::new()).unwrap();

        assert_eq!(token, verified.decoded);
    }
}
