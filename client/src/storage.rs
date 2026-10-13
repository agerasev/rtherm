use rtherm_common::{ChannelId, Measurements, Point};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Connection, SqliteConnection, SqlitePool,
};
use std::{
    path::Path,
    time::{Duration, SystemTime},
};

/// A single uploader consumes this queue. No database lock is held during HTTP requests.
pub struct Storage {
    pool: SqlitePool,
}

pub struct Batch {
    pub measurements: Measurements,
    pub len: usize,
    last_id: i64,
}

impl Storage {
    pub async fn open(path: &Path) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .synchronous(SqliteSynchronous::Full)
                    .busy_timeout(Duration::from_secs(5)),
            )
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS pending_readings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel TEXT NOT NULL,
                value REAL NOT NULL,
                time_seconds INTEGER NOT NULL CHECK(time_seconds >= 0),
                time_nanos INTEGER NOT NULL CHECK(time_nanos BETWEEN 0 AND 999999999)
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Commit the whole sampling cycle before making it available to the uploader.
    pub async fn store(&self, measurements: &Measurements) -> Result<(), sqlx::Error> {
        let mut connection = self.pool.acquire().await?;
        let result = Self::store_cycle(&mut connection, measurements).await;
        if result.is_err() {
            // SQLITE_FULL/IOERR can roll back behind SQLx's transaction tracker.
            // Reopen after any failed cycle so later retries get a clean connection.
            let _ = connection.close().await;
        }
        result
    }

    async fn store_cycle(
        connection: &mut SqliteConnection,
        measurements: &Measurements,
    ) -> Result<(), sqlx::Error> {
        let mut transaction = connection.begin().await?;
        for (channel, points) in measurements {
            for point in points {
                let time = point
                    .time
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
                let seconds =
                    i64::try_from(time.as_secs()).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
                sqlx::query(
                    "INSERT INTO pending_readings (channel, value, time_seconds, time_nanos)
                     VALUES (?, ?, ?, ?)",
                )
                .bind(channel.as_ref())
                .bind(point.value)
                .bind(seconds)
                .bind(i64::from(time.subsec_nanos()))
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await
    }

    pub async fn load(&self, limit: u16) -> Result<Batch, sqlx::Error> {
        let rows: Vec<(i64, String, f64, i64, i64)> = sqlx::query_as(
            "SELECT id, channel, value, time_seconds, time_nanos
             FROM pending_readings ORDER BY id LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        let mut batch = Batch {
            measurements: Measurements::new(),
            len: rows.len(),
            last_id: 0,
        };
        for (id, channel, value, seconds, nanos) in rows {
            let channel =
                ChannelId::try_from(channel).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            let time = SystemTime::UNIX_EPOCH
                .checked_add(Duration::new(seconds as u64, nanos as u32))
                .ok_or_else(|| sqlx::Error::Protocol("Invalid buffered timestamp".into()))?;
            batch
                .measurements
                .entry(channel)
                .or_default()
                .push(Point { value, time });
            batch.last_id = id;
        }
        Ok(batch)
    }

    /// IDs increase monotonically, so readings sampled during upload cannot be deleted here.
    pub async fn remove(&self, batch: &Batch) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM pending_readings WHERE id <= ?")
            .bind(batch.last_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn count(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM pending_readings")
            .fetch_one(&self.pool)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestDir(std::path::PathBuf);
    impl TestDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "rtherm-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn readings(values: &[f64]) -> Measurements {
        [(
            ChannelId::try_from("Water").unwrap(),
            values
                .iter()
                .map(|&value| Point {
                    value,
                    time: SystemTime::UNIX_EPOCH + Duration::new(12345, 6789),
                })
                .collect(),
        )]
        .into()
    }

    #[tokio::test]
    async fn survives_reopen_and_removes_only_acknowledged_batch() {
        let dir = TestDir::new();
        let path = dir.0.join("queue.sqlite3");
        let storage = Storage::open(&path).await.unwrap();
        storage.store(&readings(&[1., 2., 3.])).await.unwrap();
        storage.pool.close().await;
        let storage = Storage::open(&path).await.unwrap();
        assert_eq!(storage.count().await.unwrap(), 3);
        let batch = storage.load(2).await.unwrap();
        assert_eq!(batch.len, 2);
        let points = batch.measurements.values().next().unwrap();
        assert_eq!(
            points.iter().map(|p| p.value).collect::<Vec<_>>(),
            vec![1., 2.]
        );
        assert_eq!(
            points[0].time,
            SystemTime::UNIX_EPOCH + Duration::new(12345, 6789)
        );
        storage.store(&readings(&[4.])).await.unwrap();
        storage.remove(&batch).await.unwrap();
        storage.pool.close().await;
        let storage = Storage::open(&path).await.unwrap();
        let remaining = storage.load(10).await.unwrap();
        assert_eq!(
            remaining
                .measurements
                .values()
                .next()
                .unwrap()
                .iter()
                .map(|p| p.value)
                .collect::<Vec<_>>(),
            vec![3., 4.]
        );
        storage.remove(&remaining).await.unwrap();
        storage.pool.close().await;
        let storage = Storage::open(&path).await.unwrap();
        assert_eq!(storage.count().await.unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
                .fetch_one(&storage.pool)
                .await
                .unwrap(),
            "ok"
        );
        storage.pool.close().await;
    }

    #[tokio::test]
    async fn failed_cycle_is_rolled_back_and_can_be_retried() {
        let dir = TestDir::new();
        let storage = Storage::open(&dir.0.join("queue.sqlite3")).await.unwrap();
        sqlx::query("CREATE TRIGGER fail_insert BEFORE INSERT ON pending_readings WHEN NEW.value = 2 BEGIN SELECT RAISE(ABORT, 'simulated write failure'); END")
            .execute(&storage.pool).await.unwrap();
        let measurements = readings(&[1., 2., 3.]);
        assert!(storage.store(&measurements).await.is_err());
        assert_eq!(storage.count().await.unwrap(), 0);
        sqlx::query("DROP TRIGGER fail_insert")
            .execute(&storage.pool)
            .await
            .unwrap();
        storage.store(&measurements).await.unwrap();
        assert_eq!(storage.count().await.unwrap(), 3);
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
                .fetch_one(&storage.pool)
                .await
                .unwrap(),
            "wal"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA synchronous")
                .fetch_one(&storage.pool)
                .await
                .unwrap(),
            2
        );
        storage.pool.close().await;
    }

    #[tokio::test]
    async fn full_database_keeps_committed_rows_and_recovers_after_limit_is_lifted() {
        let dir = TestDir::new();
        let storage = Storage::open(&dir.0.join("queue.sqlite3")).await.unwrap();
        storage.store(&readings(&[42.])).await.unwrap();
        let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
        sqlx::query(&format!("PRAGMA max_page_count = {pages}"))
            .execute(&storage.pool)
            .await
            .unwrap();
        let cycle = readings(&vec![1.; 1000]);
        let err = storage.store(&cycle).await.unwrap_err();
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("13")
        );
        assert_eq!(storage.count().await.unwrap(), 1);
        sqlx::query("PRAGMA max_page_count = 1000")
            .execute(&storage.pool)
            .await
            .unwrap();
        storage.store(&cycle).await.unwrap();
        assert_eq!(storage.count().await.unwrap(), 1001);
        storage.pool.close().await;
    }

    #[tokio::test]
    async fn failed_cleanup_retains_acknowledged_rows_until_retry() {
        let dir = TestDir::new();
        let storage = Storage::open(&dir.0.join("queue.sqlite3")).await.unwrap();
        storage.store(&readings(&[1., 2., 3.])).await.unwrap();
        let batch = storage.load(2).await.unwrap();
        sqlx::query("CREATE TRIGGER fail_delete BEFORE DELETE ON pending_readings BEGIN SELECT RAISE(ABORT, 'simulated cleanup failure'); END")
            .execute(&storage.pool).await.unwrap();
        assert!(storage.remove(&batch).await.is_err());
        assert_eq!(storage.count().await.unwrap(), 3);
        sqlx::query("DROP TRIGGER fail_delete")
            .execute(&storage.pool)
            .await
            .unwrap();
        storage.remove(&batch).await.unwrap();
        assert_eq!(storage.count().await.unwrap(), 1);
        storage.pool.close().await;
    }
}
