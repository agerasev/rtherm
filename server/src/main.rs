use actix_web::{get, post, web, App, HttpServer, Result};
use rtherm_common::{Measurement, ProvideRequest};
use std::{collections::HashMap, sync::RwLock};

#[derive(Default)]
struct AppData {
    sensors: RwLock<HashMap<String, Measurement>>,
}

#[get("/")]
async fn hello(data: web::Data<AppData>) -> Result<String> {
    Ok(format!("{:?}", &*data.sensors.read().unwrap()))
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
        println!("measurement provided: {}, {:?}", id, measurement);
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
            .service(hello)
            .service(provide)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
