use crate::coincheck::model::NewOrder;
use crate::coincheck::model::{Balance, OpenOrder, Order, OrderType};
use crate::coincheck::request::OrdersPostRequest;
use crate::coincheck::response::OrdersCancelStatusGetResponse;
use crate::coincheck::response::OrdersDeleteResponse;
use crate::coincheck::response::{
    BalanceGetResponse, OrdersOpensGetResponse, OrdersPostResponse, OrdersRateGetResponse,
};

use std::collections::HashMap;
use std::error::Error;
use std::time::SystemTime;

use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use serde::de::DeserializeOwned;
use serde::Serialize;

const BASE_URL: &str = "https://coincheck.com";

#[derive(Debug)]
pub struct Client {
    client: reqwest::Client,
    access_key: String,
    secret_key: String,
}

impl Client {
    pub fn new(access_key: &str, secret_key: &str) -> Result<Client, reqwest::Error> {
        let client = reqwest::Client::builder().build()?;
        Ok(Client {
            client: client,
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
        })
    }

    pub async fn get_exchange_orders_rate(
        &self,
        t: OrderType,
        pair: &str,
    ) -> Result<f64, Box<dyn Error>> {
        let url = format!("{}{}", BASE_URL, "/api/exchange/orders/rate");
        let params = [("order_type", t.to_str()), ("pair", pair), ("amount", "1")];
        let body = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json::<OrdersRateGetResponse>()
            .await?;
        let rate = body.rate.parse::<f64>()?;
        Ok(rate)
    }

    pub async fn post_exchange_orders(&self, req: &NewOrder) -> Result<Order, Box<dyn Error>> {
        let url = format!("{}{}", BASE_URL, "/api/exchange/orders");
        let req_body = OrdersPostRequest::new(req)?;
        let res = self
            .post_request_with_auth::<OrdersPostRequest, OrdersPostResponse>(&url, req_body)
            .await?;
        Ok(res.to_model()?)
    }

    pub async fn get_exchange_orders_opens(&self) -> Result<Vec<OpenOrder>, Box<dyn Error>> {
        let url = format!("{}{}", BASE_URL, "/api/exchange/orders/opens");
        let body = self
            .get_request_with_auth::<OrdersOpensGetResponse>(&url)
            .await?;
        let mut res: Vec<OpenOrder> = Vec::new();
        for o in body.orders {
            res.push(o.to_model()?);
        }

        Ok(res)
    }

    pub async fn delete_exchange_orders(&self, id: u64) -> Result<u64, Box<dyn Error>> {
        let url = format!("{}{}{}", BASE_URL, "/api/exchange/orders/", id);
        let body = self
            .delete_request_with_auth::<OrdersDeleteResponse>(&url)
            .await?;
        Ok(body.id)
    }

    pub async fn get_exchange_orders_cancel_status(&self, id: u64) -> Result<bool, Box<dyn Error>> {
        let url: String = format!(
            "{}{}{}",
            BASE_URL, "/api/exchange/orders/cancel_status?id=", id
        );
        let body = self
            .get_request_with_auth::<OrdersCancelStatusGetResponse>(&url)
            .await?;
        Ok(body.cancel)
    }

    pub async fn get_accounts_balance(&self) -> Result<HashMap<String, Balance>, Box<dyn Error>> {
        let url: String = format!("{}{}", BASE_URL, "/api/accounts/balance");
        let body = self
            .get_request_with_auth::<BalanceGetResponse>(&url)
            .await?;
        Ok(body.to_map()?)
    }

    async fn get_request_with_auth<T: DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, Box<dyn Error>> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();
        let signature = make_signature(nonce, &url, "", &self.secret_key);

        let res_text = self
            .client
            .get(url)
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", format!("{}", nonce))
            .header("ACCESS-SIGNATURE", signature)
            .send()
            .await?
            .text()
            .await?;

        match serde_json::from_str::<T>(&res_text) {
            Ok(res) => Ok(res),
            Err(_) => Err(Box::new(crate::error::Error::ParseError(res_text))),
        }
    }

    async fn post_request_with_auth<T, U>(&self, url: &str, body: T) -> Result<U, Box<dyn Error>>
    where
        T: Serialize,
        U: DeserializeOwned,
    {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();
        let json = serde_json::to_string(&body)?;
        let signature = make_signature(nonce, &url, &json, &self.secret_key);

        let res_text = self
            .client
            .post(url)
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", format!("{}", nonce))
            .header("ACCESS-SIGNATURE", signature)
            .body(json)
            .send()
            .await?
            .text()
            .await?;

        match serde_json::from_str::<U>(&res_text) {
            Ok(res) => Ok(res),
            Err(_) => Err(Box::new(crate::error::Error::ParseError(res_text))),
        }
    }

    async fn delete_request_with_auth<T: DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, Box<dyn Error>> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();
        let signature = make_signature(nonce, &url, "", &self.secret_key);

        let res_text = self
            .client
            .delete(url)
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", format!("{}", nonce))
            .header("ACCESS-SIGNATURE", signature)
            .send()
            .await?
            .text()
            .await?;

        match serde_json::from_str::<T>(&res_text) {
            Ok(res) => Ok(res),
            Err(_) => Err(Box::new(crate::error::Error::ParseError(res_text))),
        }
    }
}

fn make_signature(nonce: u128, url: &str, body: &str, secret_key: &str) -> String {
    let key = PKey::hmac(secret_key.as_bytes()).unwrap();
    let mut signer = Signer::new(MessageDigest::sha256(), &key).unwrap();
    let v = format!("{}{}{}", nonce, url, body);
    signer.update(&v.as_bytes()).unwrap();
    let bb = signer.sign_to_vec().unwrap();
    bb.iter()
        .fold("".to_owned(), |s, b| format!("{}{:02x}", s, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_signature() {
        assert_eq!(
            make_signature(12345, "https://example.com", "hoge=foo", "abcdefg"),
            "65a5d4bf76d4266e2f56582c31ca3e9ac163c80745e84357ead5a2899a37e218"
        );
    }
}
