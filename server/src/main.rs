mod config;
mod db;
mod recepient;
mod statistics;
mod storage;
#[cfg(feature = "telegram")]
mod telegram;

use self::{
    config::{Config, DashboardConfig, HttpConfig},
    db::Db,
    recepient::{AnyRecepient, Recepient},
    statistics::Statistics,
};
use actix_files as fs;
use actix_web::{web, App, HttpServer, Responder, Result};
use config::StorageType;
use db::DbStorage;
use rtherm_common::{ChannelId, ProvideRequest};
use serde::{Deserialize, Serialize};
use sqlx::Connection;
use statistics::ChannelStatistics;
use std::time::SystemTime;
use std::{collections::HashMap, env, io};
use storage::{AnyStorage, FileStorage, MemStorage};
use tokio::sync::{Mutex, RwLock};

#[cfg(feature = "postgres")]
async fn postgres_connection(
    config: &self::config::PostgresConfig,
) -> sqlx::postgres::PgConnection {
    loop {
        let res = sqlx::postgres::PgConnection::connect(&format!(
            "postgres://{}:{}@{}/rtherm",
            config.user, config.password, config.host
        ))
        .await;
        match res {
            Ok(conn) => break conn,
            Err(e) => {
                if let sqlx::Error::Database(e) = &e {
                    if e.code().as_deref() == Some("57P03") {
                        log::info!("Waiting for database to start ...");
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                }
                panic!("{e}")
            }
        }
    }
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
    let mut info = Statistics::default();

    #[cfg(feature = "postgres")]
    if let Some(db_config) = config.db.as_ref().and_then(|db| db.postgres.as_ref()) {
        let conn = postgres_connection(db_config).await;
        let mut db = Db::new(conn).await.unwrap();
        match db.recent().await {
            Ok(readings) => info.update(readings),
            Err(err) => log::error!("Cannot restore dashboard history: {err}"),
        }
        recepients.push(AnyRecepient::new(db));
        log::info!("Postgres database connected");
    }

    #[cfg(feature = "sqlite")]
    if let Some(db_config) = config.db.as_ref().and_then(|db| db.sqlite.as_ref()) {
        let conn = sqlite_connection(db_config).await;
        let mut db = Db::new(conn).await.unwrap();
        match db.recent().await {
            Ok(readings) => info.update(readings),
            Err(err) => log::error!("Cannot restore dashboard history: {err}"),
        }
        recepients.push(AnyRecepient::new(db));
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
                    DbStorage::new(postgres_connection(db_config).await)
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

    serve(config.http, recepients, info, config.dashboard)
        .await
        .unwrap();
}

struct State<R: Recepient> {
    info: RwLock<Statistics>,
    recepient: Mutex<R>,
    dashboard: DashboardConfig,
}

#[derive(Deserialize)]
struct DashboardQuery {
    #[serde(default = "default_history_hours")]
    hours: u8,
}
fn default_history_hours() -> u8 {
    24
}

#[derive(Serialize)]
struct DashboardChannel {
    last: Option<rtherm_common::Point>,
    statistics: ChannelStatistics,
    points: Vec<rtherm_common::Point>,
    gaps: Vec<u64>,
}

#[derive(Serialize)]
struct DashboardResponse {
    devices: Vec<config::DeviceConfig>,
    channels: HashMap<ChannelId, DashboardChannel>,
    generated_at: u64,
    history_hours: u8,
    stale_after_seconds: u64,
}

pub async fn serve<R: Recepient + Send + 'static>(
    config: HttpConfig,
    recepient: R,
    info: Statistics,
    dashboard: DashboardConfig,
) -> io::Result<()> {
    let state = web::Data::new(State {
        info: RwLock::new(info),
        recepient: Mutex::new(recepient),
        dashboard,
    });
    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/summary", web::get().to(summary::<R>))
            .route("/dashboard", web::get().to(dashboard_data::<R>))
            .route("/provide", web::post().to(provide::<R>))
            .service(fs::Files::new("/", "./static").index_file("index.html"))
    })
    .bind((config.host, config.port))?;
    log::info!("Running HTTP server");
    server.run().await
}

async fn provide<R: Recepient>(
    data: web::Data<State<R>>,
    request: web::Json<ProvideRequest>,
) -> Result<&'static str> {
    let request = request.into_inner();
    let mut recepient = data.recepient.lock().await;
    log::debug!("Measurements obtained: {:?}", request);
    data.info.write().await.update(request.measurements.clone());
    for err in recepient.update(request.measurements).await {
        log::error!("Recepient update error: {err}");
    }
    Ok("Accepted")
}

async fn summary<R: Recepient>(data: web::Data<State<R>>) -> Result<impl Responder> {
    let channels: HashMap<_, _> = data
        .info
        .read()
        .await
        .channels
        .iter()
        .map(|(id, history)| (id.clone(), history.statistics()))
        .collect();
    Ok(web::Json(channels))
}

async fn dashboard_data<R: Recepient>(
    data: web::Data<State<R>>,
    query: web::Query<DashboardQuery>,
) -> Result<impl Responder> {
    if ![1, 6, 24].contains(&query.hours) {
        return Err(actix_web::error::ErrorBadRequest(
            "hours must be 1, 6, or 24",
        ));
    }
    let now = SystemTime::now();
    let channels = data
        .info
        .read()
        .await
        .channels
        .iter()
        .map(|(id, history)| {
            let recent = history.recent(query.hours, now);
            (
                id.clone(),
                DashboardChannel {
                    last: history.statistics().last,
                    statistics: recent.statistics(),
                    points: recent.chart_points(),
                    gaps: recent.gaps(),
                },
            )
        })
        .collect();
    Ok(web::Json(DashboardResponse {
        devices: data.dashboard.devices.clone(),
        channels,
        generated_at: now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        history_hours: query.hours,
        stale_after_seconds: 240,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::StatusCode, test};
    use rtherm_common::Point;

    #[actix_web::test]
    async fn dashboard_does_not_wait_for_delivery_and_validates_range() {
        let mut info = Statistics::default();
        info.update(
            [(
                ChannelId::try_from("Water").unwrap(),
                vec![Point {
                    value: 12.5,
                    time: SystemTime::now(),
                }],
            )]
            .into(),
        );
        let state = web::Data::new(State {
            info: RwLock::new(info),
            recepient: Mutex::new(Vec::<AnyRecepient>::new()),
            dashboard: DashboardConfig::default(),
        });
        let app = test::init_service(App::new().app_data(state.clone()).route(
            "/dashboard",
            web::get().to(dashboard_data::<Vec<AnyRecepient>>),
        ))
        .await;
        let _pending_upload = state.recepient.lock().await;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            test::call_service(
                &app,
                test::TestRequest::get()
                    .uri("/dashboard?hours=1")
                    .to_request(),
            ),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(
            body["channels"]["Water"]["points"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(body["history_hours"], 1);
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/dashboard?hours=255")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
