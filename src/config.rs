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
    // 外部サービスの処理待ち間隔（秒）
    pub external_service_wait_interval_sec: u64,
    // デモモード（有効にすると注文を出さない）
    pub demo_mode: bool,

    // 加重移動平均の期間（短期）
    pub wma_period_short: usize,
    // 加重移動平均の期間（長期）
    pub wma_period_long: usize,

    // レジスタンスライン作成に必要な期間
    pub resistance_line_period: usize,
    // レジスタンスライン作成のオフセット
    pub resistance_line_offset: usize,
    // レジスタンスライン幅（上側）の比率
    pub resistance_line_width_ratio_upper: f64,
    // レジスタンスライン幅（下側）の比率
    pub resistance_line_width_ratio_lower: f64,

    // サポートライン作成に必要な期間（長期）
    pub support_line_period_long: usize,
    // サポートライン作成に必要な期間（短期）
    pub support_line_period_short: usize,
    // サポートライン作成のオフセット
    pub support_line_offset: usize,
    // サポートライン幅（上側）の比率
    pub support_line_width_ratio_upper: f64,
    // サポートライン幅（下側）の比率
    pub support_line_width_ratio_lower: f64,

    // 出来高の短期幅
    pub volume_period_short: usize,
    // 許容する板の厚さ（短期出来高に対する割合を指定）
    pub order_books_size_ratio: f64,

    // リバウンドの判定期間（どのくらい過去を見るか）
    pub rebound_check_period: usize,
    // 注文1回に使う資金（残高JPYに対する割合を指定）
    pub funds_ratio_per_order: f64,
    // 注文1回あたりの目標利益率（買い注文時のJPYに対する割合を指定）
    pub profit_ratio_per_order: f64,
    // 注文1回あたりの目標利益率 下降トレンド時（買い注文時のJPYに対する割合を指定）
    pub profit_ratio_per_order_on_down_trend: f64,

    // ポジション保有期間の最大時間（分）
    pub hold_limit_minutes: i64,
    // ナンピン基準レート（約定待ちレートに対する割合を指定）
    pub avg_down_rate_ratio: f64,
    // ナンピン基準レート 保有期間切れ時（約定待ちレートに対する割合を指定）
    pub avg_down_rate_ratio_on_holding_expired: f64,

    // 損切り基準レート（約定待ちレートに対する割合を指定）
    pub loss_cut_rate_ratio: f64,
    // スキップ基準レート（約定待ちレートに対する割合を指定）
    pub entry_skip_rate_ratio: f64,
    // 売られすぎと判断する売り出来高しきい値（昨日の合計出来高に対する割合を指定）
    pub over_sell_volume_ratio: f64,
    // 最低限必要な取引頻度（0.0〜1.0）
    pub required_trade_frequency_ratio: f64,

    // 最低限残すロット数
    pub keep_lot: f64,

    // 取引所関連
    pub exchange_access_key: String,
    pub exchange_secret_key: String,

    // DB関連
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user_name: String,
    pub db_password: String,

    // Slack関連
    pub slack_url: String,
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
