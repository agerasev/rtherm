use rtherm::{config::Config, http, telegram::Telegram};
use std::env;

#[tokio::main]
async fn main() {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };
    let telegram = Telegram::new(config.telegram).await;
    http::serve(config.http, telegram).await.unwrap();
}
