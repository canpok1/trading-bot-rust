use crate::error;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use chrono::DateTime;
use chrono::FixedOffset;
use serde::Deserialize;

#[derive(Debug)]
pub struct Pair {
    pub key: String,
    pub settlement: String,
}

impl Pair {
    pub fn new(p: &str) -> Result<Pair, Box<dyn Error>> {
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
}

impl OrderType {
    pub fn parse(t: &str) -> Result<OrderType, Box<dyn Error>> {
        match t {
            "sell" => Ok(OrderType::Sell),
            "buy" => Ok(OrderType::Buy),
            _ => Err(Box::new(error::Error::ParseError(t.to_owned()))),
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            OrderType::Sell => "sell",
            OrderType::Buy => "buy",
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct OrdersRateResponse {
    pub success: bool,
    pub error: Option<String>,
    pub rate: String,
    pub price: String,
    pub amount: String,
}

#[derive(Deserialize, Debug)]
pub struct OrdersOpensResponse {
    pub success: bool,
    pub error: Option<String>,
    pub orders: Vec<OpenOrderResponse>,
}

#[derive(Deserialize, Debug)]
pub struct OpenOrderResponse {
    pub id: u64,
    pub rate: String,
    pub pending_amount: String,
    pub pending_market_buy_amount: Option<String>,
    pub order_type: String,
    pub stop_loss_rate: Option<String>,
    pub pair: String,
    pub created_at: String,
}

impl OpenOrderResponse {
    pub fn to_model(&self) -> Result<OpenOrder, Box<dyn Error>> {
        Ok(OpenOrder {
            id: self.id,
            rate: self.rate.parse()?,
            pending_amount: self.pending_amount.parse()?,
            pending_market_buy_amount: if let Some(amount) = &self.pending_market_buy_amount {
                Some(amount.parse()?)
            } else {
                None
            },
            order_type: OrderType::parse(&self.order_type)?,
            pair: self.pair.to_owned(),
            created_at: DateTime::parse_from_rfc3339(&self.created_at)?,
        })
    }
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

#[derive(Deserialize, Debug)]
pub struct AccountsBalanceResponse {
    pub success: bool,
    pub error: Option<String>,
    pub jpy: String,
    pub btc: String,
    pub etc: String,
    pub fct: String,
    pub mona: String,
    pub jpy_reserved: String,
    pub btc_reserved: String,
    pub etc_reserved: String,
    pub fct_reserved: String,
    pub mona_reserved: String,
}

impl AccountsBalanceResponse {
    pub fn to_map(&self) -> Result<HashMap<String, Balance>, Box<dyn Error>> {
        let mut map: HashMap<String, Balance> = HashMap::new();

        map.insert(
            "jpy".to_owned(),
            Balance {
                amount: self.jpy.parse()?,
                reserved: self.jpy_reserved.parse()?,
            },
        );

        map.insert(
            "btc".to_owned(),
            Balance {
                amount: self.btc.parse()?,
                reserved: self.btc_reserved.parse()?,
            },
        );

        map.insert(
            "etc".to_owned(),
            Balance {
                amount: self.etc.parse()?,
                reserved: self.etc_reserved.parse()?,
            },
        );

        map.insert(
            "fct".to_owned(),
            Balance {
                amount: self.fct.parse()?,
                reserved: self.fct_reserved.parse()?,
            },
        );

        map.insert(
            "mona".to_owned(),
            Balance {
                amount: self.mona.parse()?,
                reserved: self.mona_reserved.parse()?,
            },
        );

        Ok(map)
    }
}

#[derive(Debug)]
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
