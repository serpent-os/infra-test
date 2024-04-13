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
    #[sqlx(rename = "account_id", try_from = "Uuid")]
    pub account: account::Id,
    pub description: String,
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub kind: Kind,
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
        if let Kind::Builder(ext) = &self.kind {
            Some(ext)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct Tokens {
    pub account_token: Option<String>,
    pub api_token: Option<String>,
}

impl Tokens {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", content = "extension", rename_all = "kebab-case")]
pub enum Kind {
    Hub,
    RepositoryManager,
    Builder(builder::Extension),
}

impl Kind {
    pub fn role(&self) -> Role {
        match self {
            Self::Hub => Role::Hub,
            Self::RepositoryManager => Role::RepositoryManager,
            Self::Builder(_) => Role::Builder,
        }
    }

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
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Extension {
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
