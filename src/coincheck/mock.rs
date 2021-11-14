use crate::bot::model::TradeInfo;
use crate::bot::model::TradeInfoParam;
use crate::coincheck::client::Client;
use crate::coincheck::model::Pair;
use crate::coincheck::model::{Balance, NewOrder, OpenOrder, Order, OrderBooks, OrderType};
use crate::config::Config;
use crate::error::MyError::{EmptyCollection, KeyNotFound};
use crate::error::MyResult;
use crate::mysql::model::Market;
use async_trait::async_trait;
use chrono::FixedOffset;
use chrono::TimeZone;
use std::collections::HashMap;

#[derive(Debug)]
pub struct SimulationClient {
    markets: HashMap<String, Vec<Market>>,
}

impl SimulationClient {
    pub fn new() -> MyResult<SimulationClient> {
        Ok(SimulationClient {
            markets: HashMap::new(),
        })
    }

    pub fn add_market(&mut self, market: &Market) -> MyResult<()> {
        let pair = market.pair.clone();
        if !self.markets.contains_key(&pair) {
            self.markets.insert(pair.to_string(), vec![]);
        }
        self.markets
            .get_mut(&pair.to_string())
            .unwrap()
            .push(market.clone());
        Ok(())
    }

    pub fn get_market(&self, pair: &str) -> MyResult<Option<Market>> {
        if !self.markets.contains_key(pair) {
            return Err(Box::new(KeyNotFound {
                key: pair.to_owned(),
                collection_name: "markets".to_owned(),
            }));
        }

        if let Some(market) = self.markets.get(pair).unwrap().iter().last() {
            Ok(Some(market.clone()))
        } else {
            Ok(None)
        }
    }

    pub fn make_info(&self, pair: &str, config: &Config) -> MyResult<TradeInfo> {
        if let Some(market) = self.get_market(pair)? {
            let mut param = TradeInfoParam::default();

            param.pair = Pair::new(pair)?;
            param.support_line_period_long = config.support_line_period_long;
            param.support_line_period_short = config.support_line_period_short;
            param.support_line_offset = config.support_line_offset;
            param.resistance_line_period = config.resistance_line_period;
            param.resistance_line_offset = config.resistance_line_offset;

            param
                .sell_rates
                .insert(pair.to_string(), market.ex_rate_sell);
            param.buy_rate = market.ex_rate_buy;
            param.sell_rate_histories.push(market.ex_rate_sell);
            param.sell_volumes.push(market.ex_volume_sell);
            param.buy_volumes.push(market.ex_volume_buy);

            Ok(param.build()?)
        } else {
            Err(Box::new(EmptyCollection("markets".to_string())))
        }
    }
}

#[async_trait]
impl Client for SimulationClient {
    async fn get_order_books(&self, _pair: &str) -> MyResult<OrderBooks> {
        Ok(OrderBooks::default())
    }

    async fn get_exchange_orders_rate(
        &self,
        t: OrderType,
        pair: &str,
        _amount: f64,
    ) -> MyResult<f64> {
        if let Some(market) = self.get_market(pair)? {
            if t == OrderType::Buy || t == OrderType::MarketBuy {
                Ok(market.ex_rate_buy)
            } else {
                Ok(market.ex_rate_sell)
            }
        } else {
            Err(Box::new(EmptyCollection("markets".to_string())))
        }
    }

    async fn post_exchange_orders(&self, req: &NewOrder) -> MyResult<Order> {
        let tz = FixedOffset::east(9 * 60 * 60);
        if let Some(market) = self.get_market(&req.pair)? {
            // TODO 実装
            Ok(Order {
                id: 0,
                rate: None,
                amount: None,
                order_type: req.order_type.clone(),
                pair: Pair::new(&req.pair)?,
                created_at: tz.from_utc_datetime(&market.recorded_at),
            })
        } else {
            Err(Box::new(EmptyCollection("markets".to_string())))
        }
    }

    async fn get_exchange_orders_opens(&self) -> MyResult<Vec<OpenOrder>> {
        // TODO 実装
        Ok(vec![])
    }

    async fn delete_exchange_orders(&self, _id: u64) -> MyResult<u64> {
        // TODO 実装
        Ok(0)
    }

    async fn get_exchange_orders_cancel_status(&self, _id: u64) -> MyResult<bool> {
        Ok(true)
    }

    async fn get_accounts_balance(&self) -> MyResult<HashMap<String, Balance>> {
        // TODO 実装
        Ok(HashMap::new())
    }
}
