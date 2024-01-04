use chrono::{DateTime, Utc};
use derive_more::{Display, From, Into};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use strum::EnumString;

use crate::{crypto::EncodedPublicKey, database, Database};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, From, Into, Display)]
pub struct Id(i64);

#[derive(Debug, Clone, FromRow)]
pub struct Account {
    #[sqlx(rename = "account_id", try_from = "i64")]
    pub id: Id,
    #[sqlx(rename = "type", try_from = "&'a str")]
    pub kind: Kind,
    pub username: String,
    pub email: String,
    #[sqlx(try_from = "String")]
    pub public_key: EncodedPublicKey,
}

impl Account {
    pub async fn get(db: &Database, id: Id) -> Result<Self, database::Error> {
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

    pub async fn service(
        db: &Database,
        username: impl ToString,
        email: impl ToString,
        public_key: EncodedPublicKey,
    ) -> Result<Id, database::Error> {
        #[derive(FromRow)]
        struct Row {
            #[sqlx(rename = "account_id", try_from = "i64")]
            id: Id,
        }

        let Row { id } = sqlx::query_as(
            "
            INSERT INTO account
            (
              type,
              username,
              email,
              public_key
            )
            VALUES (?,?,?,?)
            RETURNING (account_id);
            ",
        )
        .bind(Kind::Service.to_string())
        .bind(username.to_string())
        .bind(email.to_string())
        .bind(public_key.to_string())
        .fetch_one(&db.pool)
        .await?;

        Ok(id)
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
pub struct BearerToken {
    pub encoded: String,
    pub expiration: DateTime<Utc>,
}

impl BearerToken {
    pub async fn set(
        db: &Database,
        id: Id,
        encoded: impl ToString,
        expiration: DateTime<Utc>,
    ) -> Result<(), database::Error> {
        sqlx::query(
            "
            INSERT INTO bearer_token
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

    pub async fn get(db: &Database, id: Id) -> Result<BearerToken, database::Error> {
        let token: BearerToken = sqlx::query_as(
            "
            SELECT
              encoded,
              expiration
            FROM bearer_token
            WHERE account_id = ?;
            ",
        )
        .bind(id.0)
        .fetch_one(&db.pool)
        .await?;

        Ok(token)
    }
}
