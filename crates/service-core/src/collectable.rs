use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Log,
    JsonManifest,
    BinaryManifest,
    Package,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Collectable {
    #[serde(rename = "type")]
    pub kind: Kind,
    pub uri: String,
    pub sha256sum: String,
}
