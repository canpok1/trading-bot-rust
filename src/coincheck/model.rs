use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;

#[derive(Deserialize, Debug)]
pub enum OrderType {
    Sell,
    Buy,
}
impl OrderType {
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
    pub rate: String,
    pub price: String,
    pub amount: String,
}

#[derive(Deserialize, Debug)]
pub struct OrdersOpensResponse {
    success: bool,
    orders: Vec<OpenOrder>,
}

#[derive(Deserialize, Debug)]
pub struct OpenOrder {
    id: u64,
    rate: Option<f64>,
    pending_amount: f64,
    pending_market_buy_amount: Option<f64>,
    order_type: OrderType,
    pair: String,
    created_at: String,
}

#[derive(Deserialize, Debug)]
pub struct AccountsBalanceResponse {
    pub success: bool,
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
