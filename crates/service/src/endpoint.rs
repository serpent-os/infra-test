use std::fmt;
use std::str::FromStr;

use derive_more::From;
use http::Uri;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize)]
pub struct Endpoint {
    pub id: Id,
    #[serde(with = "http_serde::uri")]
    pub host_address: Uri,
    pub status: Status,
    pub error: Option<String>,
    pub bearer_token: Option<String>,
    pub api_token: Option<String>,
    pub account: account::Id,
    pub extension: Option<Extension>,
}

impl Endpoint {
    pub async fn save(&self, db: &Database) -> Result<(), database::Error> {
        let builder = self.builder();

        let admin_email = builder.as_ref().map(|b| &b.admin_email);
        let admin_name = builder.as_ref().map(|b| &b.admin_name);
        let description = builder.as_ref().map(|b| &b.description);
        let work_status = builder.as_ref().map(|b| b.work_status.to_string());

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
              admin_email,
              admin_name,
              description,
              work_status
            )
            VALUES (?,?,?,?,?,?,?,?,?,?,?)
            ON CONFLICT(account_id) DO UPDATE SET 
              host_address=excluded.host_address,
              status=excluded.status,
              error=excluded.error,
              bearer_token=excluded.bearer_token,
              api_token=excluded.api_token,
              account_id=excluded.account_id,
              admin_email=excluded.admin_email,
              admin_name=excluded.admin_name,
              description=excluded.description,
              work_status=excluded.work_status;
            ",
        )
        .bind(self.id.0)
        .bind(self.host_address.to_string())
        .bind(self.status.to_string())
        .bind(&self.error)
        .bind(&self.bearer_token)
        .bind(&self.api_token)
        .bind(self.account.uuid())
        .bind(admin_email)
        .bind(admin_name)
        .bind(description)
        .bind(work_status)
        .execute(&db.pool)
        .await?;

        Ok(())
    }

    pub async fn list(db: &Database) -> Result<Vec<Endpoint>, database::Error> {
        #[derive(Debug, Clone, FromRow)]
        struct Row {
            #[sqlx(rename = "endpoint_id", try_from = "Uuid")]
            id: Id,
            #[sqlx(try_from = "&'a str")]
            host_address: Uri,
            #[sqlx(try_from = "&'a str")]
            status: Status,
            error: Option<String>,
            bearer_token: Option<String>,
            api_token: Option<String>,
            #[sqlx(rename = "account_id", try_from = "Uuid")]
            account: account::Id,
            admin_email: Option<String>,
            admin_name: Option<String>,
            description: Option<String>,
            work_status: Option<builder::WorkStatus>,
        }

        let endpoints: Vec<Row> = sqlx::query_as(
            "
            SELECT
              endpoint_id,
              host_address,
              status,
              error,
              bearer_token,
              api_token,
              account_id,
              admin_email,
              admin_name,
              description,
              work_status
            FROM endpoint;
            ",
        )
        .fetch_all(&db.pool)
        .await?;

        Ok(endpoints
            .into_iter()
            .map(|row| {
                // TODO: This is broke, we need a field which
                // defines the extension type
                let extension = row
                    .admin_email
                    .zip(row.admin_name)
                    .zip(row.description)
                    .zip(row.work_status)
                    .map(|(((admin_email, admin_name), description), work_status)| {
                        Extension::Builder(builder::Extension {
                            admin_email,
                            admin_name,
                            description,
                            work_status,
                        })
                    });

                Endpoint {
                    id: row.id,
                    host_address: row.host_address,
                    status: row.status,
                    error: row.error,
                    bearer_token: row.bearer_token,
                    api_token: row.api_token,
                    account: row.account,
                    extension,
                }
            })
            .collect())
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

impl From<proto::EnrollmentRole> for Role {
    fn from(role: proto::EnrollmentRole) -> Self {
        match role {
            proto::EnrollmentRole::Builder => Self::Builder,
            proto::EnrollmentRole::RepositoryManager => Self::RepositoryManager,
            proto::EnrollmentRole::Hub => Self::Hub,
        }
    }
}

impl From<Role> for proto::EnrollmentRole {
    fn from(role: Role) -> Self {
        match role {
            Role::Builder => Self::Builder,
            Role::RepositoryManager => Self::RepositoryManager,
            Role::Hub => Self::Hub,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Extension {
    Builder(builder::Extension),
}

pub mod builder {
    use serde::Serialize;
    use sqlx::{error::BoxDynError, sqlite::SqliteValueRef, Sqlite};

    #[derive(Debug, Clone, Serialize)]
    pub struct Extension {
        pub admin_email: String,
        pub admin_name: String,
        pub description: String,
        pub work_status: WorkStatus,
    }

    #[derive(Debug, Clone, Copy, strum::Display, strum::EnumString, Serialize)]
    #[serde(rename_all = "kebab-case")]
    #[strum(serialize_all = "kebab-case")]
    pub enum WorkStatus {
        Idle,
        Running,
    }

    impl<'a> sqlx::Decode<'a, Sqlite> for WorkStatus {
        fn decode(value: SqliteValueRef<'a>) -> Result<Self, BoxDynError> {
            Ok(<&str as sqlx::Decode<Sqlite>>::decode(value)?.parse()?)
        }
    }

    impl sqlx::Type<Sqlite> for WorkStatus {
        fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
            <&str as sqlx::Type<Sqlite>>::type_info()
        }

        fn compatible(ty: &<Sqlite as sqlx::Database>::TypeInfo) -> bool {
            <&str as sqlx::Type<Sqlite>>::compatible(ty)
        }
    }
}

mod proto {
    use tonic::include_proto;

    include_proto!("endpoint");
}
