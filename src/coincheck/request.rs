use crate::coincheck::model;
use serde::Serialize;
use std::error::Error;

// 新規注文
// POST /api/exchange/orders
#[derive(Serialize, Debug)]
pub struct OrdersPostRequest {
    pub pair: String,
    pub order_type: String,
    pub rate: Option<String>,
    pub amount: Option<String>,
    pub market_buy_amount: Option<String>,
    pub stop_loss_rate: Option<String>,
}

impl OrdersPostRequest {
    pub fn new(order: &model::NewOrder) -> Result<OrdersPostRequest, Box<dyn Error>> {
        let rate = if let Some(v) = order.rate {
            Some(format!("{:.3}", v))
        } else {
            None
        };
        let amount = if let Some(v) = order.amount {
            Some(format!("{:.3}", v))
        } else {
            None
        };
        let market_buy_amount = if let Some(v) = order.market_buy_amount {
            Some(format!("{:.3}", v))
        } else {
            None
        };

        Ok(OrdersPostRequest {
            pair: order.pair.to_owned(),
            order_type: order.order_type.to_str().to_owned(),
            rate: rate,
            amount: amount,
            market_buy_amount: market_buy_amount,
            stop_loss_rate: None,
        })
    }
}
