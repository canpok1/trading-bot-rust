use crate::coincheck;
use crate::coincheck::model::Balance;
use log::{debug, info};
use std::error::Error;
use std::{thread, time};

#[derive(Debug)]
struct ExchangeInfo {
    pair: String,
    sell_rate: f64,
    buy_rate: f64,
    balance_key: Balance,
    balance_settlement: Balance,
}

#[derive(Debug)]
struct ActionParam {}

#[derive(Debug)]
pub struct Bot {
    pub config: crate::config::Config,
    pub coincheck_client: coincheck::client::Client,
}

impl Bot {
    pub fn wait(&self) -> Result<(), Box<dyn Error>> {
        let d = time::Duration::from_secs(self.config.interval_sec);
        debug!("wait ... [{:?}]", d);
        thread::sleep(d);
        Ok(())
    }

    pub async fn trade(&self) -> Result<(), Box<dyn Error>> {
        let info = self.fetch().await?;
        info!("{:?}", info);

        self.upsert(&info)?;
        let params = self.make_params(&info)?;
        for param in params.iter() {
            self.action(param)?;
        }
        Ok(())
    }

    async fn fetch(&self) -> Result<ExchangeInfo, Box<dyn Error>> {
        let sell_rate = self
            .coincheck_client
            .get_exchange_orders_rate(coincheck::model::OrderType::Sell, &self.config.target_pair)
            .await?;
        let buy_rate = self
            .coincheck_client
            .get_exchange_orders_rate(coincheck::model::OrderType::Buy, &self.config.target_pair)
            .await?;

        let balances = self.coincheck_client.get_accounts_balance().await?;
        let key = self.config.key_currency();
        let balance_key = balances
            .get(&key)
            .ok_or(format!("balance {} is empty", key))?;

        let settlement = self.config.settlement_currency();
        let balance_settlement = balances
            .get(&settlement)
            .ok_or(format!("balance {} is empty", settlement))?;

        let info = ExchangeInfo {
            pair: self.config.target_pair.clone(),
            sell_rate: sell_rate,
            buy_rate: buy_rate,
            balance_key: Balance {
                amount: balance_key.amount,
                reserved: balance_key.reserved,
            },
            balance_settlement: Balance {
                amount: balance_settlement.amount,
                reserved: balance_settlement.reserved,
            },
        };
        Ok(info)
    }

    fn upsert(&self, _info: &ExchangeInfo) -> Result<(), Box<dyn Error>> {
        debug!("called upsert");
        Ok(())
    }

    fn make_params(&self, _info: &ExchangeInfo) -> Result<Vec<ActionParam>, Box<dyn Error>> {
        debug!("called make_params");
        Ok(vec![])
    }

    fn action(&self, _param: &ActionParam) -> Result<(), Box<dyn Error>> {
        debug!("called action");
        Ok(())
    }
}
