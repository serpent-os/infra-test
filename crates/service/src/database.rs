use std::path::Path;

use sqlx::{sqlite::SqliteConnectOptions, Pool, Sqlite, Transaction};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Database {
    pub pool: Pool<Sqlite>,
}

impl Database {
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .read_only(false)
            .foreign_keys(true);

        Self::connect(options).await
    }

    async fn connect(options: SqliteConnectOptions) -> Result<Self, Error> {
        let pool = sqlx::SqlitePool::connect_with(options).await?;

        sqlx::migrate!("src/database/migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn transaction(&self) -> Result<Transaction<'static, Sqlite>, Error> {
        Ok(self.pool.begin().await?)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlx")]
    Sqlx(#[from] sqlx::Error),
    #[error("sqlx migration")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

// /// Decode from a database type using [`Encoding::decode`]
// #[derive(Debug, Clone, Copy)]
// pub struct Decoder<T>(pub T);

// /// A trait to define an encoding between a sql type and rust type
// pub trait Encoding<'a>: Sized {
//     type Encoded: ToOwned;
//     type Error;

//     fn decode(encoded: Self::Encoded) -> Result<Self, Self::Error>;
//     fn encode(&'a self) -> Self::Encoded;
// }

// impl<'r, T, U, E> sqlx::Decode<'r, Sqlite> for Decoder<T>
// where
//     T: Encoding<'r, Encoded = U, Error = E>,
//     U: sqlx::Decode<'r, Sqlite> + ToOwned,
//     E: std::error::Error + Send + Sync + 'static,
// {
//     fn decode(
//         value: <Sqlite as sqlx::database::HasValueRef<'r>>::ValueRef,
//     ) -> Result<Self, sqlx::error::BoxDynError> {
//         Ok(T::decode(U::decode(value)?).map(Decoder)?)
//     }
// }

// impl<T, U, E> Type<Sqlite> for Decoder<T>
// where
//     T: Encoding<'static, Encoded = U, Error = E>,
//     U: ToOwned + Type<Sqlite>,
// {
//     fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
//         U::type_info()
//     }

//     fn compatible(ty: &<Sqlite as sqlx::Database>::TypeInfo) -> bool {
//         U::compatible(ty)
//     }
// }
