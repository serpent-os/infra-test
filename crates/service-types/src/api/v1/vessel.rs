use serde::{Deserialize, Serialize};
use service_core::operation;

use crate::Collectable;

operation!(Build, POST, "vessel/build", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: BuildRequestBody);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRequestBody {
    #[serde(alias = "taskID")]
    pub task_id: u64,
    pub collectables: Vec<Collectable>,
}
