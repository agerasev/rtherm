use rtherm::{config::Config, db::Db, http, telegram::Telegram};
use sqlx::Connection;
use std::env;

#[tokio::main]
async fn main() {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };

    #[cfg(feature = "postgres")]
    let client =
        sqlx::postgres::PgConnection::connect("postgres://postgres:password@localhost/test")
            .await
            .unwrap();
    #[cfg(feature = "sqlite")]
    let client = sqlx::sqlite::SqliteConnection::connect("../data/database.db")
        .await
        .unwrap();
    #[cfg(any(
        all(feature = "postgres", feature = "sqlite"),
        not(any(feature = "postgres", feature = "sqlite"))
    ))]
    let client: sqlx::AnyConnection =
        unimplemented!("One of `postgres` or `sqlite` features should be selected");
    let db = Db::new(client).await.unwrap();

    //let telegram = Telegram::new(config.telegram).await;
    http::serve(config.http, db).await.unwrap();
}
