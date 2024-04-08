use std::fmt;

use chrono::{DateTime, Utc};
use derive_more::From;
use log::debug;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use strum::EnumString;
use thiserror::Error;
use uuid::Uuid;

pub use self::service::{Client, Server, Service};
use crate::{crypto::EncodedPublicKey, database, Database};

pub mod service;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, From, FromRow)]
#[serde(try_from = "&str", into = "String")]
pub struct Id(Uuid);

impl Id {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn uuid(&self) -> &Uuid {
        &self.0
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
pub struct Account {
    #[sqlx(rename = "account_id", try_from = "Uuid")]
    pub id: Id,
    #[sqlx(rename = "type", try_from = "&'a str")]
    pub kind: Kind,
    pub username: String,
    pub email: String,
    #[sqlx(try_from = "String")]
    pub public_key: EncodedPublicKey,
}

impl Account {
    pub async fn get(db: &Database, id: Id) -> Result<Self, Error> {
        let account: Account = sqlx::query_as(
            "
            SELECT
              account_id,
              type,
              username,
              email,
              public_key
            FROM account
            WHERE account_id = ?;
            ",
        )
        .bind(id.0)
        .fetch_one(&db.pool)
        .await?;

        Ok(account)
    }

    pub async fn lookup_with_credentials(
        db: &Database,
        username: &str,
        public_key: &EncodedPublicKey,
    ) -> Result<Self, Error> {
        let account: Account = sqlx::query_as(
            "
            SELECT
              account_id,
              type,
              username,
              email,
              public_key
            FROM account
            WHERE 
              username = ?
              AND public_key = ?
              AND (type = 'admin' OR type = 'standard');
            ",
        )
        .bind(username)
        .bind(public_key.to_string())
        .fetch_one(&db.pool)
        .await?;

        Ok(account)
    }

    pub async fn save<'c>(
        &self,
        conn: impl sqlx::Executor<'c, Database = sqlx::Sqlite>,
    ) -> Result<(), Error> {
        sqlx::query(
            "
            INSERT INTO account
            (
              account_id,
              type,
              username,
              email,
              public_key
            )
            VALUES (?,?,?,?,?)
            ON CONFLICT(account_id) DO UPDATE SET 
              type=excluded.type,
              username=excluded.username,
              email=excluded.email,
              public_key=excluded.public_key;
            ",
        )
        .bind(self.id.0)
        .bind(self.kind.to_string())
        .bind(&self.username)
        .bind(&self.email)
        .bind(self.public_key.to_string())
        .execute(conn)
        .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, strum::Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Kind {
    Admin,
    Standard,
    Bot,
    Service,
}

#[derive(Debug, Clone, FromRow)]
pub struct Token {
    pub encoded: String,
    pub expiration: DateTime<Utc>,
}

impl Token {
    pub async fn set(
        db: &Database,
        id: Id,
        encoded: impl ToString,
        expiration: DateTime<Utc>,
    ) -> Result<(), Error> {
        sqlx::query(
            "
            INSERT INTO account_token
            (
              account_id,
              encoded,
              expiration
            )
            VALUES (?,?,?);
            ",
        )
        .bind(id.0)
        .bind(encoded.to_string())
        .bind(expiration)
        .execute(&db.pool)
        .await?;

        Ok(())
    }

    pub async fn get(db: &Database, id: Id) -> Result<Token, Error> {
        let token: Token = sqlx::query_as(
            "
            SELECT
              encoded,
              expiration
            FROM account_token
            WHERE account_id = ?;
            ",
        )
        .bind(id.0)
        .fetch_one(&db.pool)
        .await?;

        Ok(token)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Admin {
    pub username: String,
    pub email: String,
    pub public_key: EncodedPublicKey,
}

/// Ensure only the provided admin account exists in the db.
pub(crate) async fn sync_admin(db: &Database, admin: Admin) -> Result<(), Error> {
    let account: Option<Id> = sqlx::query_as(
        "
        SELECT 
          account_id
        FROM account
        WHERE 
          type = 'admin'
          AND username = ?
          AND email = ?
          AND public_key = ?;
        ",
    )
    .bind(&admin.username)
    .bind(&admin.email)
    .bind(admin.public_key.to_string())
    .fetch_optional(&db.pool)
    .await?;

    if account.is_some() {
        return Ok(());
    }

    let mut transaction = db.transaction().await?;

    sqlx::query(
        "
        DELETE FROM account
        WHERE type = 'admin';
        ",
    )
    .execute(transaction.as_mut())
    .await?;

    Account {
        id: Id::generate(),
        kind: Kind::Admin,
        username: admin.username.clone(),
        email: admin.email.clone(),
        public_key: admin.public_key.clone(),
    }
    .save(transaction.as_mut())
    .await?;

    transaction.commit().await?;

    debug!(
        "Admin account set as username {}, public_key {}",
        &admin.username, &admin.public_key
    );

    Ok(())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("database")]
    Database(#[from] database::Error),
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Error::Database(database::Error::from(error))
    }
}

mod proto {
    use tonic::include_proto;

    include_proto!("account");
}
