use crate::{config::HttpConfig, db::DbHandle};
use actix_files as fs;
use actix_web::{web, App, HttpServer, Responder, Result};
use rtherm_common::ProvideRequest;
use std::io;

#[derive(Default)]
struct AppData {
    db: DbHandle,
}

async fn read(data: web::Data<AppData>) -> Result<impl Responder> {
    let sensors = data.db.stats();
    Ok(web::Json(sensors))
}

async fn provide(
    data: web::Data<AppData>,
    request: web::Json<ProvideRequest>,
) -> Result<&'static str> {
    let request = request.into_inner();
    for (sensor, measurement) in request.measurements {
        let id = format!("{}.{}", request.source, sensor);
        println!("Measurement obtained from '{}': {:?}", id, measurement);
        data.db.update(id, measurement);
    }
    Ok("Accepted")
}

pub async fn serve(config: HttpConfig) -> io::Result<()> {
    let prefix = move |path: &str| format!("{}{}", config.prefix, path);
    let state = web::Data::new(AppData::default());
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
