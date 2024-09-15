use crate::{config::HttpConfig, recepient::Recepient, statistics::Statistics};
use actix_files as fs;
use actix_web::{web, App, HttpServer, Responder, Result};
use rtherm_common::ProvideRequest;
use std::{collections::HashMap, io};
use tokio::sync::Mutex;

struct State<R: Recepient> {
    info: Statistics,
    recepient: R,
}

async fn info<R: Recepient>(data: web::Data<Mutex<State<R>>>) -> Result<impl Responder> {
    let info = data
        .lock()
        .await
        .info
        .channels
        .iter()
        .map(|(id, values)| (id.clone(), values.statistics()))
        .collect::<HashMap<_, _>>();

    Ok(web::Json(info))
}

async fn provide<R: Recepient>(
    data: web::Data<Mutex<State<R>>>,
    request: web::Json<ProvideRequest>,
) -> Result<&'static str> {
    let request = request.into_inner();

    let mut guard = data.lock().await;
    let State { info, recepient } = &mut *guard;
    for (name, meas) in request.measurements {
        let channel_id = format!("{}.{}", request.source, name);
        println!("Measurement obtained from '{}': {:?}", channel_id, meas);
        info.update(channel_id.clone(), meas);
        recepient.update(channel_id, meas).await.unwrap();
    }

    Ok("Accepted")
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
            .route(&prefix("/info"), web::get().to(info::<R>))
            .route(&prefix("/provide"), web::post().to(provide::<R>))
            .service(fs::Files::new(&prefix("/"), "./static"))
    })
    .bind((config.host, config.port))?;
    println!("Running HTTP server");
    server.run().await
}
