mod config;
mod db;
mod http;
mod telegram;

use crate::{config::Config, db::Db};
use std::env;
use tokio::task;

#[tokio::main]
async fn main() {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };

    let db = Db::default().handle();

    task::spawn(telegram::run(config.telegram, db.clone()));
    http::serve(config.http, db).await.unwrap();
}
