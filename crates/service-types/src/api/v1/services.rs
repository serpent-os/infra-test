use serde::{Deserialize, Serialize};
use service_core::operation;

use crate::endpoint::enrollment;

operation!(
    Enroll,
    POST,
    "services/enrol",
    req: EnrollRequestBody
);

operation!(
    Accept,
    POST,
    "services/accept",
    NOT_EXPIRED | BEARER_TOKEN | SERVICE_ACCOUNT,
    req: AcceptRequestBody
);

operation!(
    Decline,
    POST,
    "services/decline",
    NOT_EXPIRED | BEARER_TOKEN | SERVICE_ACCOUNT
);

operation!(
    RefreshToken,
    GET,
    "services/refresh_token",
    NOT_EXPIRED | BEARER_TOKEN | SERVICE_ACCOUNT,
    resp: String
);

operation!(
    RefreshIssueToken,
    GET,
    "services/refresh_issue_token",
    BEARER_TOKEN | SERVICE_ACCOUNT,
    resp: String
);

#[derive(Debug, Serialize, Deserialize)]
pub struct EnrollRequestBody {
    pub request: enrollment::Request,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptRequestBody {
    pub request: enrollment::Request,
}
