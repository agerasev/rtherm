use actix_files as fs;
use actix_web::{get, post, web, App, HttpServer, Responder, Result};
use rtherm_common::{Measurement, ProvideRequest};
use std::{collections::HashMap, sync::RwLock};

#[derive(Default)]
struct AppData {
    sensors: RwLock<HashMap<String, Measurement>>,
}

#[get("/sensors")]
async fn get_sensors(data: web::Data<AppData>) -> Result<impl Responder> {
    let sensors = data.sensors.read().unwrap();
    Ok(web::Json(sensors.clone()))
}

#[post("/provide")]
async fn provide(
    data: web::Data<AppData>,
    request: web::Json<ProvideRequest>,
) -> Result<&'static str> {
    let request = request.into_inner();
    let mut sensors = data.sensors.write().unwrap();
    for (sensor, measurement) in request.measurements {
        let id = format!("{}.{}", request.source, sensor);
        sensors.insert(id, measurement);
    }
    Ok("Accepted")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let state = web::Data::new(AppData::default());
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(get_sensors)
            .service(provide)
            .service(fs::Files::new("/static", "./static").show_files_listing())
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
