use serde::{Deserialize, Serialize};
use service_core::operation;

use crate::Remote;

operation!(Build, POST, "avalanche/build", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: BuildRequestBody);

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildRequestBody {
    pub request: PackageBuild,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageBuild {
    #[serde(rename = "buildID")]
    pub build_id: u64,
    pub uri: String,
    pub commit_ref: String,
    pub relative_path: String,
    pub build_architecture: String,
    #[serde(rename = "collections")]
    pub remotes: Vec<Remote>,
}
