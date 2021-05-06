use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub bot_name: String,
    pub target_pair: String,
    pub interval_sec: u64,
    pub exchange_access_key: String,
    pub exchange_secret_key: String,
}

impl Config {
    pub fn key_currency(&self) -> String {
        let splited: Vec<&str> = self.target_pair.split('_').collect();
        splited[0].to_string()
    }
    pub fn settlement_currency(&self) -> String {
        let splited: Vec<&str> = self.target_pair.split('_').collect();
        splited[1].to_string()
    }
}
