mod config;
mod db;
mod http;

use crate::config::Config;
use std::env;

#[tokio::main]
async fn main() {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };

    http::serve(config.http).await.unwrap();
}
