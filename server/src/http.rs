use crate::{
    config::HttpConfig,
    db::{DbHandle, Sensor},
};
use actix_files as fs;
use actix_web::{web, App, HttpServer, Responder, Result};
use rtherm_common::ProvideRequest;
use std::{
    collections::{hash_map::Entry, HashMap},
    io,
};

struct AppData {
    db: DbHandle,
}

async fn read(data: web::Data<AppData>) -> Result<impl Responder> {
    let sensors = data
        .db
        .read()
        .await
        .sensors
        .iter()
        .map(|(id, sensor)| (id.clone(), sensor.stats()))
        .collect::<HashMap<_, _>>();
    Ok(web::Json(sensors))
}

async fn provide(
    data: web::Data<AppData>,
    request: web::Json<ProvideRequest>,
) -> Result<&'static str> {
    let request = request.into_inner();
    let mut db = data.db.write().await;
    for (name, meas) in request.measurements {
        let id = format!("{}.{}", request.source, name);
        println!("Measurement obtained from '{}': {:?}", id, meas);
        match db.sensors.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(Sensor::new(meas));
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().update(meas);
            }
        }
    }
    Ok("Accepted")
}

pub async fn serve(config: HttpConfig, db: DbHandle) -> io::Result<()> {
    let prefix = move |path: &str| format!("{}{}", config.prefix, path);
    let state = web::Data::new(AppData { db });
    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route(&prefix("/sensors"), web::get().to(read))
            .route(&prefix("/provide"), web::post().to(provide))
            .service(fs::Files::new(&prefix("/static"), "./static"))
    })
    .bind((config.host, config.port))?;
    server.run().await
}
