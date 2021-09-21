use chrono::Utc;
use trading_bot_rust::bot::action::ActionBehavior;
use trading_bot_rust::bot::base::Bot;
use trading_bot_rust::config::Config;
use trading_bot_rust::strategy::base::StrategyType;
use trading_bot_rust::{coincheck, mysql, slack, strategy};

use env_logger;
use log::{error, info};

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

    let coincheck_cli: coincheck::client::DefaultClient;
    match coincheck::client::DefaultClient::new(
        &config.exchange_access_key,
        &config.exchange_secret_key,
    ) {
        Ok(cli) => {
            coincheck_cli = cli;
        }
        Err(err) => {
            error!("{}", err);
            return;
        }
    }

    let mysql_cli: mysql::client::DefaultClient;
    match mysql::client::DefaultClient::new(
        &config.db_user_name,
        &config.db_password,
        &config.db_host,
        config.db_port,
        &config.db_name,
    ) {
        Ok(cli) => {
            mysql_cli = cli;
        }
        Err(err) => {
            error!("{}", err);
            return;
        }
    }

    let slack_cli: slack::client::DefaultClient;
    match slack::client::DefaultClient::new(&config.slack_url) {
        Ok(cli) => {
            slack_cli = cli;
        }
        Err(err) => {
            error!("{}", err);
            return;
        }
    }

    info!("===========================================");
    info!("bot_name   : {}", config.bot_name);
    info!("pair       : {}", config.target_pair);
    info!("interval   : {}sec", config.interval_sec);
    info!("rate period: {}min", config.rate_period_minutes);
    info!("demo mode  : {}", config.demo_mode);
    info!("===========================================");

    let strategy_type = StrategyType::Scalping;
    let strategy = match strategy_type {
        StrategyType::Scalping => strategy::scalping::ScalpingStrategy { config: &config },
    };

    let action_behavior = ActionBehavior {
        config: &config,
        slack_client: &slack_cli,
        mysql_client: &mysql_cli,
        coincheck_client: &coincheck_cli,
    };

    let bot = Bot {
        config: &config,
        coincheck_client: &coincheck_cli,
        mysql_client: &mysql_cli,
        slack_client: &slack_cli,
        strategy: &strategy,
        action_behavior: &action_behavior,
    };

    loop {
        let now = Utc::now();
        if let Err(err) = bot.trade(&now).await {
            error!("{:?}", err);
        }
        if let Err(err) = bot.wait() {
            error!("{:?}", err);
        }
    }
}
