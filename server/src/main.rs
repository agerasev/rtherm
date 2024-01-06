mod config;

use crate::config::Config;
use actix_files as fs;
use actix_web::{web, App, HttpServer, Responder, Result};
use rtherm_common::{Measurement, ProvideRequest};
use std::{collections::HashMap, env, io, sync::RwLock};

#[derive(Default)]
struct AppData {
    sensors: RwLock<HashMap<String, Measurement>>,
}

async fn get_sensors(data: web::Data<AppData>) -> Result<impl Responder> {
    let sensors = data.sensors.read().unwrap();
    Ok(web::Json(sensors.clone()))
}

async fn provide(
    data: web::Data<AppData>,
    request: web::Json<ProvideRequest>,
) -> Result<&'static str> {
    let request = request.into_inner();
    let mut sensors = data.sensors.write().unwrap();
    for (sensor, measurement) in request.measurements {
        let id = format!("{}.{}", request.source, sensor);
        println!("Measurement obtained from '{}': {:?}", id, measurement);
        sensors.insert(id, measurement);
    }
    Ok("Accepted")
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };

    let prefix = move |path: &str| format!("{}{}", config.http.prefix, path);
    let state = web::Data::new(AppData::default());
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route(&prefix("/sensors"), web::get().to(get_sensors))
            .route(&prefix("/provide"), web::post().to(provide))
            .service(fs::Files::new(&prefix("/static"), "./static"))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
