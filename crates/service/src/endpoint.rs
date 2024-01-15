use std::fmt;
use std::str::FromStr;

use derive_more::From;
use http::Uri;
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::FromRow;
use uuid::Uuid;

pub use self::enrollment::Enrollment;
pub use self::service::{Client, Server, Service};
use crate::{account, database, Database};

pub mod enrollment;
pub mod service;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, From)]
#[serde(try_from = "&str", into = "String")]
pub struct Id(Uuid);

impl Id {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl FromStr for Id {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse::<Uuid>().map(Id)
    }
}

impl<'a> TryFrom<&'a str> for Id {
    type Error = uuid::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        value.parse::<Uuid>().map(Id)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Id> for String {
    fn from(id: Id) -> Self {
        id.to_string()
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Endpoint {
    #[sqlx(rename = "endpoint_id", try_from = "Uuid")]
    pub id: Id,
    #[serde(with = "http_serde::uri")]
    #[sqlx(try_from = "&'a str")]
    pub host_address: Uri,
    #[sqlx(try_from = "&'a str")]
    pub status: Status,
    pub error: Option<String>,
    pub bearer_token: Option<String>,
    pub api_token: Option<String>,
    #[sqlx(rename = "account_id", try_from = "Uuid")]
    pub account: account::Id,
    #[sqlx(json)]
    pub extension: Option<Extension>,
}

impl Endpoint {
    pub async fn save(&self, db: &Database) -> Result<(), database::Error> {
        sqlx::query(
            "
            INSERT INTO endpoint
            (
              endpoint_id,
              host_address,
              status,
              error,
              bearer_token,
              api_token,
              account_id,
              extension
            )
            VALUES (?,?,?,?,?,?,?,?)
            ON CONFLICT(account_id) DO UPDATE SET 
              host_address=excluded.host_address,
              status=excluded.status,
              error=excluded.error,
              bearer_token=excluded.bearer_token,
              api_token=excluded.api_token,
              account_id=excluded.account_id,
              extension=excluded.extension;
            ",
        )
        .bind(self.id.0)
        .bind(self.host_address.to_string())
        .bind(self.status.to_string())
        .bind(&self.error)
        .bind(&self.bearer_token)
        .bind(&self.api_token)
        .bind(self.account.uuid())
        .bind(Json(&self.extension))
        .execute(&db.pool)
        .await?;

        Ok(())
    }

    pub async fn list(db: &Database) -> Result<Vec<Endpoint>, database::Error> {
        let endpoints: Vec<Endpoint> = sqlx::query_as(
            "
            SELECT
              endpoint_id,
              host_address,
              status,
              error,
              bearer_token,
              api_token,
              account_id,
              extension
            FROM endpoint;
            ",
        )
        .fetch_all(&db.pool)
        .await?;

        Ok(endpoints)
    }

    pub async fn delete(&self, db: &Database) -> Result<(), database::Error> {
        sqlx::query(
            "
            DELETE FROM endpoint
            WHERE endpoint_id = ?;
            ",
        )
        .bind(self.id.0)
        .execute(&db.pool)
        .await?;

        Ok(())
    }

    pub fn builder(&self) -> Option<&builder::Extension> {
        if let Some(Extension::Builder(ext)) = &self.extension {
            Some(ext)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, strum::Display, strum::EnumString, Serialize)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Status {
    AwaitingAcceptance,
    Failed,
    Operational,
    Forbidden,
    Unreachable,
}

#[derive(Debug, Clone, Copy, strum::Display, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Role {
    Hub,
    RepositoryManager,
    Builder,
}

impl Role {
    pub fn service_name(&self) -> &'static str {
        match self {
            Role::Hub => "summit",
            Role::RepositoryManager => "vessel",
            Role::Builder => "avalanche",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Extension {
    Builder(builder::Extension),
}

pub mod builder {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Extension {
        pub admin_email: String,
        pub admin_name: String,
        pub description: String,
        pub work_status: WorkStatus,
    }

    #[derive(Debug, Clone, Copy, strum::Display, strum::EnumString, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    #[strum(serialize_all = "kebab-case")]
    pub enum WorkStatus {
        Idle,
        Running,
    }
}

mod proto {
    use tonic::include_proto;

    use crate::Role;

    include_proto!("endpoint");

    impl From<EnrollmentRole> for Role {
        fn from(role: EnrollmentRole) -> Self {
            match role {
                EnrollmentRole::Builder => Self::Builder,
                EnrollmentRole::RepositoryManager => Self::RepositoryManager,
                EnrollmentRole::Hub => Self::Hub,
            }
        }
    }

    impl From<Role> for EnrollmentRole {
        fn from(role: Role) -> Self {
            match role {
                Role::Builder => Self::Builder,
                Role::RepositoryManager => Self::RepositoryManager,
                Role::Hub => Self::Hub,
            }
        }
    }
}
