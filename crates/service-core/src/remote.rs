use http::Uri;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remote {
    #[serde(rename = "indexURI", with = "http_serde::uri")]
    pub index_uri: Uri,
    pub name: String,
    pub priority: u64,
}
