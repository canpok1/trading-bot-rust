pub mod bot;
pub mod coincheck;
pub mod config;

use crate::bot::base::Bot;
use crate::config::Config;
use env_logger;
use log::error;

#[tokio::main]
async fn main() {
    env_logger::init();

    let config: Config;
    match envy::from_env::<Config>() {
        Ok(val) => config = val,
        Err(err) => {
            error!("{}", err);
            return;
        }
    }

    let coincheck_cli: coincheck::client::Client;
    match coincheck::client::Client::new(&config.exchange_access_key, &config.exchange_secret_key) {
        Ok(cli) => {
            coincheck_cli = cli;
        }
        Err(err) => {
            error!("{}", err);
            return;
        }
    }

    let bot = Bot {
        config: config,
        coincheck_client: coincheck_cli,
    };

    loop {
        if let Err(err) = bot.trade().await {
            error!("{}", err);
        }
        if let Err(err) = bot.wait() {
            error!("{}", err);
        }
    }
}
