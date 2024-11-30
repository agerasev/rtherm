mod config;
mod db;
mod recepient;
mod statistics;
mod storage;
#[cfg(feature = "telegram")]
mod telegram;

use self::{
    config::{Config, HttpConfig},
    db::Db,
    recepient::{AnyRecepient, Recepient},
    statistics::Statistics,
};
use actix_files as fs;
use actix_web::{web, App, HttpServer, Responder, Result};
use config::StorageType;
use db::DbStorage;
use rtherm_common::{ChannelId, ProvideRequest};
use sqlx::Connection;
use statistics::ChannelStatistics;
use std::{collections::HashMap, env, io};
use storage::{AnyStorage, FileStorage, MemStorage};
use tokio::sync::Mutex;

#[cfg(feature = "postgres")]
async fn postgres_connection(
    config: &self::config::PostgresConfig,
) -> sqlx::postgres::PgConnection {
    sqlx::postgres::PgConnection::connect(&format!(
        "postgres://{}:{}@{}/rtherm",
        config.user, config.password, config.host
    ))
    .await
    .unwrap()
}

#[cfg(feature = "sqlite")]
async fn sqlite_connection(config: &self::config::SqliteConfig) -> sqlx::sqlite::SqliteConnection {
    sqlx::sqlite::SqliteConnection::connect(&config.path)
        .await
        .unwrap()
}

#[tokio::main]
async fn main() {
    let config = {
        let path = env::args()
            .nth(1)
            .expect("Path to config must be provided as argument");
        Config::read(&path)
            .await
            .unwrap_or_else(|e| panic!("Error reading config from {path:?}: {e}"))
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let mut recepients = Vec::<AnyRecepient>::new();

    #[cfg(feature = "postgres")]
    if let Some(db_config) = config.db.as_ref().and_then(|db| db.postgres.as_ref()) {
        let conn = postgres_connection(&db_config).await;
        recepients.push(AnyRecepient::new(Db::new(conn).await.unwrap()));
        log::info!("Postgres database connected");
    }

    #[cfg(feature = "sqlite")]
    if let Some(db_config) = config.db.as_ref().and_then(|db| db.sqlite.as_ref()) {
        let conn = sqlite_connection(db_config).await;
        recepients.push(AnyRecepient::new(Db::new(conn).await.unwrap()));
        log::info!("SQLite database connected");
    }

    let storage: AnyStorage = match config.storage.type_ {
        StorageType::Mem => AnyStorage::new(MemStorage::default()),
        StorageType::Fs => AnyStorage::new(
            FileStorage::new(
                config
                    .storage
                    .path
                    .expect(r#"Storage type is set to "fs" but path is not provided"#),
            )
            .await
            .unwrap(),
        ),
        StorageType::Db => {
            let mut db_storage = None;

            #[cfg(feature = "postgres")]
            if let Some(db_config) = config.db.as_ref().and_then(|db| db.postgres.as_ref()) {
                db_storage = Some(AnyStorage::new(
                    DbStorage::new(postgres_connection(&db_config).await)
                        .await
                        .unwrap(),
                ));
            }
            #[cfg(feature = "sqlite")]
            if let Some(db_config) = config.db.as_ref().and_then(|db| db.sqlite.as_ref()) {
                db_storage = Some(AnyStorage::new(
                    DbStorage::new(sqlite_connection(db_config).await)
                        .await
                        .unwrap(),
                ));
            }

            db_storage.expect(r#"Storage type is set to "db" but no databases found"#)
        }
    };

    #[cfg(feature = "telegram")]
    if let Some(tg_config) = config.telegram {
        recepients.push(AnyRecepient::new(
            telegram::Telegram::new(tg_config, storage).await,
        ));
        log::info!("Telegram bot started");
    }

    serve(config.http, recepients).await.unwrap();
}

struct State<R: Recepient> {
    info: Statistics,
    recepient: R,
}

impl<R: Recepient> State<R> {
    fn summary(&self) -> HashMap<ChannelId, ChannelStatistics> {
        self.info
            .channels
            .iter()
            .map(|(id, values)| (id.clone(), values.statistics()))
            .collect()
    }
}

pub async fn serve<R: Recepient + Send + 'static>(
    config: HttpConfig,
    recepient: R,
) -> io::Result<()> {
    let prefix = move |path: &str| format!("{}{}", config.prefix, path);
    let state = web::Data::new(Mutex::new(State {
        info: Statistics::default(),
        recepient,
    }));
    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route(&prefix("/summary"), web::get().to(summary::<R>))
            .route(&prefix("/provide"), web::post().to(provide::<R>))
            .service(fs::Files::new(&prefix("/"), "./static").index_file("index.html"))
    })
    .bind((config.host, config.port))?;
    log::info!("Running HTTP server");
    server.run().await
}

async fn provide<R: Recepient>(
    data: web::Data<Mutex<State<R>>>,
    request: web::Json<ProvideRequest>,
) -> Result<&'static str> {
    let request = request.into_inner();
    let mut guard = data.lock().await;
    let State { info, recepient } = &mut *guard;
    log::debug!("Measurements obtained: {:?}", request);
    info.update(request.measurements.clone());
    for err in recepient.update(request.measurements).await {
        log::error!("Recepient update error: {err}");
    }
    Ok("Accepted")
}

async fn summary<R: Recepient>(data: web::Data<Mutex<State<R>>>) -> Result<impl Responder> {
    Ok(web::Json(data.lock().await.summary()))
}
