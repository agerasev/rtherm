mod config;
mod db;
mod http;
mod telegram;

use crate::{
    config::Config,
    db::{Db, DB},
};
use std::env;
use tokio::task;

#[tokio::main]
async fn main() {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };

    let db = DB.get_or_init(|| Db::default().handle()).clone();

    task::spawn(telegram::run(config.telegram, db.clone()));
    http::serve(config.http, db).await.unwrap();
}
