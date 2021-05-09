use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub bot_name: String,
    pub target_pair: String,
    pub interval_sec: u64,
    // レート取得期間
    pub rate_period_minutes: i64,
    // トレンドライン作成に必要な期間
    pub trend_line_period: usize,
    // トレンドライン作成のオフセット
    pub trend_line_offset: usize,

    // 取引所関連
    pub exchange_access_key: String,
    pub exchange_secret_key: String,

    // DB関連
    pub db_host: String,
    pub db_port: u64,
    pub db_name: String,
    pub db_user_name: String,
    pub db_password: String,
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
