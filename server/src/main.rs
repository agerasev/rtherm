mod config;
mod db;
mod recepient;
mod statistics;
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
use rtherm_common::{ChannelId, ProvideRequest};
use sqlx::Connection;
use statistics::ChannelStatistics;
use std::{collections::HashMap, env, io};
use tokio::sync::Mutex;

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
        let conn = sqlx::postgres::PgConnection::connect(&format!(
            "postgres://{}:{}@{}/rtherm",
            db_config.user, db_config.password, db_config.host
        ))
        .await
        .unwrap();
        recepients.push(AnyRecepient::new(Db::new(conn).await.unwrap()));
        log::info!("Postgres database connected");
    }

    #[cfg(feature = "sqlite")]
    if let Some(db_config) = config.db.as_ref().and_then(|db| db.sqlite.as_ref()) {
        let conn = sqlx::sqlite::SqliteConnection::connect(&db_config.path)
            .await
            .unwrap();
        recepients.push(AnyRecepient::new(Db::new(conn).await.unwrap()));
        log::info!("SQLite database connected");
    }

    #[cfg(feature = "telegram")]
    if let Some(tg_config) = config.telegram {
        recepients.push(AnyRecepient::new(telegram::Telegram::new(tg_config).await));
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
