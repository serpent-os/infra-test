use url::Url;

pub use self::service::{Client, Server, Service};
use crate::Account;

pub mod service;

#[derive(Debug, Clone, Copy)]
pub struct Id(u64);

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub id: Id,
    pub host_address: Url,
    pub status: Status,
    pub bearer_token: Option<String>,
    pub api_token: Option<String>,
    pub account: Account,
    pub extension: Option<Extension>,
}

impl Endpoint {
    pub fn avalanche(&self) -> Option<&avalanche::Extension> {
        if let Some(Extension::Avalance(ext)) = &self.extension {
            Some(ext)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Status {
    AwaitingAcceptance,
    AwaitingEnrolment,
    Failed,
    Operational,
    Forbidden,
    Unreachable,
}

#[derive(Debug, Clone)]
pub enum Extension {
    Avalance(avalanche::Extension),
}

pub mod avalanche {
    #[derive(Debug, Clone)]
    pub struct Extension {
        pub admin_email: String,
        pub admin_name: String,
        pub description: String,
        pub work_status: WorkStatus,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum WorkStatus {
        Idle,
        Running,
    }
}
