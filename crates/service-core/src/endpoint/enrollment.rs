use serde::{Deserialize, Serialize};

use crate::Role;

/// An endpoint enrollment request
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// The issuer of the request
    pub issuer: Issuer,
    /// The issueing token assigned to the service
    pub issue_token: String,
    /// The role assigned to the service
    pub role: Role,
}

/// Contains details of the service issuing the enrollment request
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issuer {
    /// Encoded public key for the issuer
    pub public_key: String,
    /// Valid callback base URL for handshakes
    pub url: String,
    /// The service issuers role, i.e. Hub
    pub role: Role,
}
