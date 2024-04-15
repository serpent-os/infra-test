//! Describe remote services and connect to them

use std::fmt;
use std::str::FromStr;

use derive_more::From;
use http::Uri;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

pub use self::service::{Client, Server, Service};
use crate::{account, database, Database, Role};

pub mod enrollment;
pub mod service;

/// Unique identifier of an [`Endpoint`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, From)]
#[serde(try_from = "&str", into = "String")]
pub struct Id(Uuid);

impl Id {
    /// Generate a new [`Id`]
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

/// Details of a remote endpoint (service) that we are connected to
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Endpoint {
    /// Unique identifier of the endpoint
    #[sqlx(rename = "endpoint_id", try_from = "Uuid")]
    pub id: Id,
    /// [`Uri`] we can reach the endpoint at
    #[serde(with = "http_serde::uri")]
    #[sqlx(try_from = "&'a str")]
    pub host_address: Uri,
    /// Current status of the endpoint
    #[sqlx(try_from = "&'a str")]
    pub status: Status,
    /// Error message, if any, due to the endpoint being in an
    /// error [`Status`]
    pub error: Option<String>,
    /// Related service account identifier for this endpoint
    #[sqlx(rename = "account_id", try_from = "Uuid")]
    pub account: account::Id,
    /// Description of the endpoint provided during enrollment
    pub description: String,
    /// Role specific data
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub kind: Kind,
}

impl Endpoint {
    /// Create or update this endpoint to the provided [`Database`]
    pub async fn save(&self, db: &Database) -> Result<(), database::Error> {
        sqlx::query(
            "
            INSERT INTO endpoint
            (
              endpoint_id,
              host_address,
              status,
              error,
              account_id,
              description,
              role,
              work_status
            )
            VALUES (?,?,?,?,?,?,?,?)
            ON CONFLICT(account_id) DO UPDATE SET 
              host_address=excluded.host_address,
              status=excluded.status,
              error=excluded.error,
              account_id=excluded.account_id,
              description=excluded.description,
              role=excluded.role,
              work_status=excluded.work_status;
            ",
        )
        .bind(self.id.0)
        .bind(self.host_address.to_string())
        .bind(self.status.to_string())
        .bind(&self.error)
        .bind(self.account.uuid())
        .bind(&self.description)
        .bind(self.kind.role().to_string())
        .bind(self.kind.work_status().map(ToString::to_string))
        .execute(&db.pool)
        .await?;

        Ok(())
    }

    /// List all endpoints from the provided [`Database`]
    pub async fn list(db: &Database) -> Result<Vec<Endpoint>, database::Error> {
        let endpoints: Vec<Endpoint> = sqlx::query_as(
            "
            SELECT
              endpoint_id,
              host_address,
              status,
              error,
              account_id,
              description,
              role,
              work_status
            FROM endpoint;
            ",
        )
        .fetch_all(&db.pool)
        .await?;

        Ok(endpoints)
    }

    /// Delete this endpoint from the provided [`Database`]
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

    /// Return [`builder`] related information if this endpoint is a [`Role::Builder`]
    pub fn builder(&self) -> Option<&builder::Extension> {
        if let Kind::Builder(ext) = &self.kind {
            Some(ext)
        } else {
            None
        }
    }
}

/// Auth tokens used to connect to the endpoint
#[derive(Debug, Clone, FromRow)]
pub struct Tokens {
    /// Token used for authentication
    pub account_token: Option<String>,
    /// Token used for authorization to hit APIs provided by the endpoint
    pub api_token: Option<String>,
}

impl Tokens {
    /// Save the tokens related to [`Id`] to the provided [`Database`]
    pub async fn save(&self, db: &Database, id: Id) -> Result<(), database::Error> {
        sqlx::query(
            "
            UPDATE endpoint
            SET
              account_token = ?,
              api_token = ?
            WHERE endpoint_id = ?;
            ",
        )
        .bind(&self.account_token)
        .bind(&self.api_token)
        .bind(id.0)
        .execute(&db.pool)
        .await?;

        Ok(())
    }

    /// Get the tokens for [`Id`] from the provided [`Database`]
    pub async fn get(db: &Database, id: Id) -> Result<Self, database::Error> {
        let tokens: Tokens = sqlx::query_as(
            "
            SELECT
              account_token,
              api_token
            FROM endpoint
            WHERE endpoint_id = ?;
            ",
        )
        .bind(id.0)
        .fetch_one(&db.pool)
        .await?;

        Ok(tokens)
    }
}

/// Status of the [`Endpoint`]
#[derive(Debug, Clone, Copy, strum::Display, strum::EnumString, Serialize)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Status {
    /// Awaiting enrollment acceptance for the endpoint
    AwaitingAcceptance,
    /// Endpoint is in a failed state
    Failed,
    /// Endpoint is enrolled and operational
    Operational,
    /// Authorization to the endpoint is forbidden
    Forbidden,
    /// Endpoint cannot be reeached
    Unreachable,
}

/// Extension details related to the [`Role`] of the endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", content = "extension", rename_all = "kebab-case")]
pub enum Kind {
    /// Hub
    Hub,
    /// Repository Manager
    RepositoryManager,
    /// Builder
    Builder(builder::Extension),
}

impl Kind {
    /// [`Role`] defined by this endpoint
    pub fn role(&self) -> Role {
        match self {
            Self::Hub => Role::Hub,
            Self::RepositoryManager => Role::RepositoryManager,
            Self::Builder(_) => Role::Builder,
        }
    }

    /// Work status of a [`Role::Builder`] endpoint
    pub fn work_status(&self) -> Option<&builder::WorkStatus> {
        if let Self::Builder(ext) = self {
            Some(&ext.work_status)
        } else {
            None
        }
    }
}

impl<'a> FromRow<'a, sqlx::sqlite::SqliteRow> for Kind {
    fn from_row(row: &'a sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        #[derive(Debug, FromRow)]
        struct Row {
            #[sqlx(try_from = "&'a str")]
            role: Role,

            // Builder fields
            work_status: Option<String>,
        }

        let row = Row::from_row(row)?;

        match (row.role, row.work_status) {
            (Role::Builder, Some(value)) => {
                let work_status = value
                    .parse()
                    .map_err(|e| sqlx::Error::Decode(Box::from(e)))?;
                Ok(Kind::Builder(builder::Extension { work_status }))
            }
            (Role::Builder, _) => Err(sqlx::Error::Decode(Box::from(
                "extension can't be null for builder endpoint",
            ))),
            (Role::Hub, _) => Ok(Kind::Hub),
            (Role::RepositoryManager, _) => Ok(Kind::RepositoryManager),
        }
    }
}

pub mod builder {
    //! Builder specific endpoint details
    use serde::{Deserialize, Serialize};

    /// Builder extension details
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Extension {
        /// Work status of the endpoint
        pub work_status: WorkStatus,
    }

    /// Work status of the builder
    #[derive(Debug, Clone, Copy, strum::Display, strum::EnumString, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    #[strum(serialize_all = "kebab-case")]
    pub enum WorkStatus {
        /// Builder is idle
        Idle,
        /// Builder is running
        Running,
    }
}

mod proto {
    use tonic::include_proto;

    use crate::Role;

    include_proto!("endpoint");

    impl From<EndpointRole> for Role {
        fn from(role: EndpointRole) -> Self {
            match role {
                EndpointRole::Builder => Self::Builder,
                EndpointRole::RepositoryManager => Self::RepositoryManager,
                EndpointRole::Hub => Self::Hub,
            }
        }
    }

    impl From<Role> for EndpointRole {
        fn from(role: Role) -> Self {
            match role {
                Role::Builder => Self::Builder,
                Role::RepositoryManager => Self::RepositoryManager,
                Role::Hub => Self::Hub,
            }
        }
    }
}
