use serde::{Deserialize, Serialize};
use service_core::operation;

operation!(ImportSucceeded, POST, "summit/importSucceeded", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: ImportBody);
operation!(ImportFailed, POST, "summit/importFailed", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: ImportBody);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportBody {
    #[serde(rename = "taskID")]
    pub task_id: u64,
}
