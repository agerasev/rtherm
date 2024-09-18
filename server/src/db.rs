use chrono::{DateTime, Local};
use rtherm_common::Measurement;
use sqlx::{Connection, Database, Encode, Error, Executor, IntoArguments, Type};

use crate::recepient::{ChannelId, Recepient};

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

    async fn update(
        &mut self,
        channel_id: ChannelId,
        meas: Measurement,
    ) -> Result<(), Self::Error> {
        sqlx::query::<C::Database>(
            "INSERT INTO Measurements (channel_id, value, time) VALUES ($1, $2, $3)",
        )
        .bind(channel_id)
        .bind(meas.value)
        .bind(DateTime::<Local>::from(meas.time))
        .execute(&mut self.client)
        .await?;
        Ok(())
    }
}
