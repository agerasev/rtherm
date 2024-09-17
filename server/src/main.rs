use rtherm::{config::Config, db::Db, http, recepient::AnyRecepient};
use sqlx::Connection;
use std::env;

#[tokio::main]
async fn main() {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };

    let mut recepients = Vec::<AnyRecepient>::new();

    #[cfg(feature = "postgres")]
    if let Some(db_config) = config.db.and_then(|db| db.postgres) {
        let conn = sqlx::postgres::PgConnection::connect(&format!(
            "postgres://{}:{}@{}/rtherm",
            db_config.user, db_config.password, db_config.host
        ))
        .await
        .unwrap();
        recepients.push(AnyRecepient::new(Db::new(conn).await.unwrap()));
        println!("Postgres database connected");
    }

    #[cfg(feature = "sqlite")]
    if let Some(db_config) = config.db.and_then(|db| db.sqlite) {
        let conn = sqlx::sqlite::SqliteConnection::connect(&db_config.path)
            .await
            .unwrap();
        recepients.push(AnyRecepient::new(Db::new(conn).await.unwrap()));
        println!("SQLite database connected");
    }

    #[cfg(feature = "telegram")]
    if let Some(tg_config) = config.telegram {
        recepients.push(AnyRecepient::new(
            rtherm::telegram::Telegram::new(tg_config).await,
        ));
        println!("Telegram bot started");
    }

    http::serve(config.http, recepients).await.unwrap();
}
