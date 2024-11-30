use chrono::{DateTime, Local};
use rtherm_common::Measurements;
use sqlx::{Connection, Database, Encode, Error, Executor, IntoArguments, Type};

use crate::recepient::Recepient;

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
