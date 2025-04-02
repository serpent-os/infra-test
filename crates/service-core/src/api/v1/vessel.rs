use serde::{Deserialize, Serialize};

use crate::{Collectable, operation};

operation!(Build, POST, "vessel/build", ACCESS_TOKEN | SERVICE_ACCOUNT | NOT_EXPIRED, req: BuildRequestBody);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRequestBody {
    #[serde(rename = "taskID")]
    pub task_id: u64,
    pub collectables: Vec<Collectable>,
}
