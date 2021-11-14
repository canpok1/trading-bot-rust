use crate::env_logger::Builder;
use trading_bot_rust::coincheck::model::Pair;
use trading_bot_rust::config::Config;
use trading_bot_rust::error::MyResult;
use trading_bot_rust::simulator::base::Simulator;

use env_logger;
use log::{error, info};

const MARKET_DATA_PATH: &str = "./market_data/markets__btc_updated_highest_price.csv";

#[tokio::main]
async fn main() {
    let mut builder = Builder::from_default_env();
    builder.format_module_path(false).init();

    match real_main("btc_jpy").await {
        Ok(_) => {
            info!("succeeded to simulation");
        }
        Err(err) => {
            error!("failed to simulation, {}", err);
        }
    }
    info!("finished simulation");
}

async fn real_main(pair_str: &str) -> MyResult<()> {
    let config: Config = envy::from_env::<Config>()?;

    let simulator: Simulator = Simulator::new(&config)?;
    let pair = Pair::new(pair_str)?;

    info!("===========================================");
    info!("start simulation");
    info!("pair:{}", pair.to_string());
    info!("===========================================");

    simulator.run(MARKET_DATA_PATH, &pair).await?;

    Ok(())
}
