use crate::error::MyError::ParseError;
use crate::error::MyResult;

use std::fmt;

use chrono::DateTime;
use chrono::FixedOffset;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Pair {
    pub key: String,
    pub settlement: String,
}

impl Pair {
    pub fn new(p: &str) -> MyResult<Pair> {
        let splited: Vec<&str> = p.split("_").collect();
        let pair = Pair {
            key: splited[0].to_string(),
            settlement: splited[1].to_string(),
        };
        Ok(pair)
    }

    pub fn to_string(&self) -> String {
        format!("{}_{}", self.key, self.settlement)
    }
}

#[derive(Deserialize, Debug)]
pub enum OrderType {
    Sell,
    Buy,
    MarketSell,
    MarketBuy,
}

impl OrderType {
    pub fn parse(t: &str) -> MyResult<OrderType> {
        match t {
            "sell" => Ok(OrderType::Sell),
            "buy" => Ok(OrderType::Buy),
            "market_sell" => Ok(OrderType::MarketSell),
            "market_buy" => Ok(OrderType::MarketBuy),
            _ => Err(Box::new(ParseError(t.to_owned()))),
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            OrderType::Sell => "sell",
            OrderType::Buy => "buy",
            OrderType::MarketSell => "market_sell",
            OrderType::MarketBuy => "market_buy",
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct OrderBooks {
    pub asks: Vec<OrderBook>,
    pub bids: Vec<OrderBook>,
}

#[derive(Deserialize, Debug)]
pub struct OrderBook {
    pub rate: f64,
    pub amount: f64,
}

#[derive(Deserialize, Debug)]
pub struct NewOrder {
    pub pair: String,
    pub order_type: OrderType,
    pub rate: Option<f64>,
    pub amount: Option<f64>,
    pub market_buy_amount: Option<f64>,
    pub stop_loss_rate: Option<f64>,
}

impl NewOrder {
    pub fn new_buy_order(pair: &Pair, rate: f64, amount: f64) -> NewOrder {
        NewOrder {
            pair: pair.to_string(),
            order_type: OrderType::Buy,
            rate: Some(rate),
            amount: Some(amount),
            market_buy_amount: None,
            stop_loss_rate: None,
        }
    }

    pub fn new_sell_order(pair: &Pair, rate: f64, amount: f64) -> NewOrder {
        NewOrder {
            pair: pair.to_string(),
            order_type: OrderType::Sell,
            rate: Some(rate),
            amount: Some(amount),
            market_buy_amount: None,
            stop_loss_rate: None,
        }
    }

    pub fn new_market_buy_order(pair: &Pair, market_buy_amount: f64) -> NewOrder {
        NewOrder {
            pair: pair.to_string(),
            order_type: OrderType::MarketBuy,
            rate: None,
            amount: None,
            market_buy_amount: Some(market_buy_amount),
            stop_loss_rate: None,
        }
    }

    pub fn new_market_sell_order(pair: &Pair, amount: f64) -> NewOrder {
        NewOrder {
            pair: pair.to_string(),
            order_type: OrderType::MarketSell,
            rate: None,
            amount: Some(amount),
            market_buy_amount: None,
            stop_loss_rate: None,
        }
    }
}

#[derive(Debug)]
pub struct Order {
    pub id: u64,
    pub rate: Option<f64>,
    pub amount: Option<f64>,
    pub order_type: OrderType,
    pub pair: Pair,
    pub created_at: DateTime<FixedOffset>,
}

#[derive(Deserialize, Debug)]
pub struct OpenOrder {
    pub id: u64,
    pub rate: f64,
    pub pending_amount: f64,
    pub pending_market_buy_amount: Option<f64>,
    pub order_type: OrderType,
    pub pair: String,
    pub created_at: DateTime<FixedOffset>,
}

#[derive(Debug, Clone)]
pub struct Balance {
    pub amount: f64,
    pub reserved: f64,
}

impl Balance {
    pub fn total(&self) -> f64 {
        self.amount + self.reserved
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "[amount:{:.3}, reserved:{:.3}]",
            self.amount, self.reserved
        )
    }
}
