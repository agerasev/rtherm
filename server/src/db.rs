use chrono::{DateTime, Local};
use rtherm_common::Measurements;
use sqlx::{
    ColumnIndex, Connection, Database, Decode, Encode, Error, Executor, IntoArguments, Row, Type,
};

use crate::{recepient::Recepient, storage::Storage};

pub struct Db<C: Connection>
where
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
    for<'q> <C::Database as Database>::Arguments<'q>: IntoArguments<'q, C::Database>,
{
    client: C,
}

impl<C: Connection> Db<C>
where
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
    for<'q> <C::Database as Database>::Arguments<'q>: IntoArguments<'q, C::Database>,
{
    pub async fn new(mut client: C) -> Result<Self, Error> {
        sqlx::query("CREATE TABLE IF NOT EXISTS Measurements (channel_id VARCHAR, value FLOAT, time TIMESTAMP)").execute(&mut client).await?;
        Ok(Self { client })
    }
}

impl<C: Connection> Recepient for Db<C>
where
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
    for<'q> <C::Database as Database>::Arguments<'q>: IntoArguments<'q, C::Database>,

    for<'q> String: Type<C::Database> + Encode<'q, C::Database>,
    for<'q> f64: Type<C::Database> + Encode<'q, C::Database>,
    for<'q> DateTime<Local>: Type<C::Database> + Encode<'q, C::Database>,
{
    type Error = Error;

    async fn update(&mut self, meas: Measurements) -> Vec<Self::Error> {
        // TODO: Use bulk insert
        let mut errors = Vec::new();
        for (channel_id, points) in meas {
            for p in points {
                if let Err(err) = sqlx::query::<C::Database>(
                    "INSERT INTO Measurements (channel_id, value, time) VALUES ($1, $2, $3)",
                )
                .bind(channel_id.to_string())
                .bind(p.value)
                .bind(DateTime::<Local>::from(p.time))
                .execute(&mut self.client)
                .await
                {
                    errors.push(err);
                }
            }
        }
        errors
    }
}

pub struct DbStorage<C: Connection>
where
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
    for<'q> <C::Database as Database>::Arguments<'q>: IntoArguments<'q, C::Database>,
{
    client: C,
}

impl<C: Connection> DbStorage<C>
where
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
    for<'q> <C::Database as Database>::Arguments<'q>: IntoArguments<'q, C::Database>,
{
    pub async fn new(mut client: C) -> Result<Self, sqlx::Error> {
        sqlx::query("CREATE TABLE IF NOT EXISTS Storage (name VARCHAR PRIMARY KEY, value BLOB)")
            .execute(&mut client)
            .await?;
        Ok(Self { client })
    }
}

impl<C: Connection> Storage for DbStorage<C>
where
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
    for<'q> <C::Database as Database>::Arguments<'q>: IntoArguments<'q, C::Database>,

    str: ColumnIndex<<C::Database as Database>::Row>,
    for<'q> String: Type<C::Database> + Encode<'q, C::Database>,
    for<'q> Vec<u8>: Type<C::Database> + Encode<'q, C::Database> + Decode<'q, C::Database>,
{
    type Error = sqlx::Error;
    async fn load(&mut self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        let rows = sqlx::query::<C::Database>("SELECT FROM Storage (name, value) WHERE name = $1")
            .bind(name.to_string())
            .fetch_all(&mut self.client)
            .await?;
        assert!(rows.len() < 2);
        match rows.get(0) {
            Some(row) => Ok(Some(row.try_get("name")?)),
            None => Ok(None),
        }
    }
    async fn store(&mut self, name: &str, value: &[u8]) -> Result<(), Self::Error> {
        sqlx::query::<C::Database>("INSERT INTO Storage (name, value) VALUES ($1, $2)")
            .bind(name.to_string())
            .bind(value.to_owned())
            .execute(&mut self.client)
            .await?;
        Ok(())
    }
}
