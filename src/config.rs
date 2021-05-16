use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    // ボット名
    pub bot_name: String,
    // 取引ペア
    pub target_pair: String,
    // 定期実行間隔（秒）
    pub interval_sec: u64,
    // レート取得期間
    pub rate_period_minutes: i64,
    // デモモード
    pub demo_mode: bool,

    // レジスタンスライン作成に必要な期間
    pub resistance_line_period: usize,
    // レジスタンスライン作成のオフセット
    pub resistance_line_offset: usize,
    // レジスタンスライン幅（上側）の比率
    pub resistance_line_width_ratio_upper: f64,
    // レジスタンスライン幅（下側）の比率
    pub resistance_line_width_ratio_lower: f64,

    // サポートライン作成に必要な期間
    pub support_line_period: usize,
    // サポートライン作成のオフセット
    pub support_line_offset: usize,
    // サポートライン幅（上側）の比率
    pub support_line_width_ratio_upper: f64,
    // サポートライン幅（下側）の比率
    pub support_line_width_ratio_lower: f64,

    // リバウンドの判定期間（どのくらい過去を見るか）
    pub rebound_check_period: usize,
    // 注文1回に使う資金（残高JPYに対する割合を指定）
    pub funds_ratio_per_order: f64,
    // 注文1回あたりの目標利益率（買い注文時のJPYに対する割合を指定）
    pub profit_ratio_per_order: f64,
    // 損切り基準レート（約定待ちレートに対する割合を指定）
    pub loss_cut_rate_ratio: f64,
    // スキップ基準レート（約定待ちレートに対する割合を指定）
    pub entry_skip_rate_ratio: f64,

    // 取引所関連
    pub exchange_access_key: String,
    pub exchange_secret_key: String,

    // DB関連
    pub db_host: String,
    pub db_port: u16,
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
