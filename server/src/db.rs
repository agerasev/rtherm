use chrono::{DateTime, Local, NaiveDateTime, Utc};
use rtherm_common::{ChannelId, Measurements, Point};
use sqlx::{
    ColumnIndex, Connection, Database, Decode, Encode, Error, Executor, IntoArguments, Row, Type,
    TypeInfo,
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

    /// Warm the dashboard from persisted readings after a server restart.
    pub async fn recent(&mut self) -> Result<Measurements, Error>
    where
        usize: ColumnIndex<<C::Database as Database>::Row>,
        for<'q> NaiveDateTime:
            Type<C::Database> + Encode<'q, C::Database> + Decode<'q, C::Database>,
        for<'q> String: Type<C::Database> + Decode<'q, C::Database>,
        for<'q> f64: Type<C::Database> + Decode<'q, C::Database>,
    {
        let since = (Utc::now() - chrono::Duration::hours(24)).naive_utc();
        let query = if C::Database::NAME == "SQLite" {
            // SQLite stores timezone-bearing timestamps as text; compare instants,
            // then normalize to UTC before decoding a timezone-free SQL timestamp.
            "SELECT channel_id, value, strftime('%Y-%m-%d %H:%M:%f', time)
             FROM Measurements WHERE julianday(time) >= julianday($1)
             AND value IS NOT NULL AND channel_id IS NOT NULL
             ORDER BY julianday(time) DESC LIMIT 200000"
        } else {
            "SELECT channel_id, value, time FROM Measurements
             WHERE time >= $1 AND value IS NOT NULL AND channel_id IS NOT NULL
             ORDER BY time DESC LIMIT 200000"
        };
        let rows = sqlx::query::<C::Database>(query)
            .bind(since)
            .fetch_all(&mut self.client)
            .await?;
        let mut measurements = Measurements::new();
        for row in rows {
            let channel: String = row.try_get(0)?;
            let value: f64 = row.try_get(1)?;
            let time: NaiveDateTime = row.try_get(2)?;
            if let Ok(channel) = ChannelId::try_from(channel) {
                if value.is_finite() {
                    measurements.entry(channel).or_default().push(Point {
                        value,
                        time: time.and_utc().into(),
                    });
                }
            }
        }
        Ok(measurements)
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

    for<'q> Vec<u8>: Type<C::Database>,
{
    pub async fn new(mut client: C) -> Result<Self, sqlx::Error> {
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS Storage (name VARCHAR PRIMARY KEY, value {})",
            <Vec<u8> as Type<C::Database>>::type_info().name(),
        ))
        .execute(&mut client)
        .await?;
        Ok(Self { client })
    }
}

impl<C: Connection> Storage for DbStorage<C>
where
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
    for<'q> <C::Database as Database>::Arguments<'q>: IntoArguments<'q, C::Database>,

    usize: ColumnIndex<<C::Database as Database>::Row>,
    for<'q> String: Type<C::Database> + Encode<'q, C::Database>,
    for<'q> Vec<u8>: Type<C::Database> + Encode<'q, C::Database> + Decode<'q, C::Database>,
{
    type Error = sqlx::Error;
    async fn load(&mut self, name: String) -> Result<Option<Vec<u8>>, Self::Error> {
        let rows = sqlx::query::<C::Database>("SELECT (value) FROM Storage WHERE name = $1")
            .bind(name)
            .fetch_all(&mut self.client)
            .await?;
        assert!(rows.len() < 2);
        match rows.first() {
            Some(row) => Ok(Some(row.try_get(0)?)),
            None => Ok(None),
        }
    }
    async fn store(&mut self, name: String, value: Vec<u8>) -> Result<(), Self::Error> {
        sqlx::query::<C::Database>(
            "INSERT INTO Storage (name, value) VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET value = $2",
        )
        .bind(name)
        .bind(value)
        .execute(&mut self.client)
        .await?;
        Ok(())
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dashboard_restores_recent_persisted_history_only() {
        let mut db = Db::new(sqlx::SqliteConnection::connect(":memory:").await.unwrap())
            .await
            .unwrap();
        let now = std::time::SystemTime::now();
        let channel = ChannelId::try_from("Water").unwrap();
        assert!(db
            .update(
                [(
                    channel.clone(),
                    vec![
                        Point {
                            value: 12.5,
                            time: now
                        },
                        Point {
                            value: 20.,
                            time: now - std::time::Duration::from_secs(90000)
                        }
                    ]
                )]
                .into()
            )
            .await
            .is_empty());
        let readings = db.recent().await.unwrap();
        assert_eq!(readings[&channel].len(), 1);
        assert_eq!(readings[&channel][0].value, 12.5);
        let restored = readings[&channel][0].time;
        assert!(
            restored
                .duration_since(now)
                .or_else(|_| now.duration_since(restored))
                .unwrap()
                < std::time::Duration::from_secs(1)
        );
    }
}
