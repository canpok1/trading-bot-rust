use crate::coincheck::model::AccountsBalanceResponse;
use crate::coincheck::model::Balance;
use crate::coincheck::model::OrderType;
use crate::coincheck::model::OrdersOpensResponse;
use crate::coincheck::model::OrdersRateResponse;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use std::collections::HashMap;
use std::error::Error;

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
            .json::<OrdersRateResponse>()
            .await?;
        let rate = body.rate.parse::<f64>()?;
        Ok(rate)
    }

    pub async fn get_exchange_orders_opens(&self) -> Result<OrdersOpensResponse, Box<dyn Error>> {
        let url: String = format!("{}{}", BASE_URL, "/api/exchange/orders/opens");
        let nonce: i64 = time::now().to_timespec().sec;
        let signature = make_signature(nonce, &url, "", &self.secret_key);

        let body = self
            .client
            .get(&url)
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", nonce)
            .header("ACCESS-SIGNATURE", signature)
            .send()
            .await?
            .json::<OrdersOpensResponse>()
            .await?;
        Ok(body)
    }

    pub async fn get_accounts_balance(&self) -> Result<HashMap<String, Balance>, Box<dyn Error>> {
        let url: String = format!("{}{}", BASE_URL, "/api/accounts/balance");
        let nonce: i64 = time::now().to_timespec().sec;
        let signature = make_signature(nonce, &url, "", &self.secret_key);

        let body = self
            .client
            .get(&url)
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", nonce)
            .header("ACCESS-SIGNATURE", signature)
            .send()
            .await?
            .json::<AccountsBalanceResponse>()
            .await?;
        Ok(body.to_map()?)
    }
}

fn make_signature(nonce: i64, url: &str, body: &str, secret_key: &str) -> String {
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
