use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Remote {
    #[serde(rename = "indexURI")]
    pub index_uri: String,
    pub name: String,
    pub priority: u32,
}
