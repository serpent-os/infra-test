use serde::{Deserialize, Serialize};

use crate::{operation, Collectable};

operation!(BuildSucceeded, POST, "summit/buildSucceeded", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: BuildBody);
operation!(BuildFailed, POST, "summit/buildFailed", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: BuildBody);

operation!(ImportSucceeded, POST, "summit/importSucceeded", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: ImportBody);
operation!(ImportFailed, POST, "summit/importFailed", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: ImportBody);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildBody {
    #[serde(rename = "taskID")]
    pub task_id: u64,
    pub collectables: Vec<Collectable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportBody {
    #[serde(rename = "taskID")]
    pub task_id: u64,
}
