use crate::coincheck::mock::SimulationClient;
use crate::coincheck::model::Pair;
use crate::config::Config;
use crate::error::MyResult;
use crate::mysql::model::Market;
use crate::simulator::model::CSVRecord;
use crate::strategy::base::Strategy;
use crate::strategy::scalping::ScalpingStrategy;
use chrono::DateTime;
use chrono::Utc;
use std::fs::File;
use std::io::BufReader;

#[derive(Debug, PartialEq)]
pub struct Simulator<'a> {
    pub config: &'a Config,
}

#[derive(Debug, PartialEq)]
pub struct SimulationResult {}

impl Simulator<'_> {
    pub fn new(config: &Config) -> MyResult<Simulator> {
        Ok(Simulator { config: config })
    }

    pub async fn run(&self, market_data_path: &str, pair: &Pair) -> MyResult<SimulationResult> {
        let mut client: SimulationClient = SimulationClient::new()?;
        let strategy = ScalpingStrategy {
            config: self.config,
        };

        let balance_jpy = 100000.0;
        let buy_jpy_per_lot = balance_jpy * self.config.funds_ratio_per_order;

        let buf = BufReader::new(File::open(market_data_path)?);
        let mut csv_reader = csv::ReaderBuilder::new().has_headers(true).from_reader(buf);
        for r in csv_reader.deserialize() {
            let record: CSVRecord = r?;
            if record.pair != pair.to_string() {
                continue;
            }
            let market = record.to_model()?;

            match self
                .run_one_step(buy_jpy_per_lot, &mut client, &strategy, &market)
                .await
            {
                Ok(_) => {}
                Err(_err) => {}
            };
        }

        Ok(SimulationResult {})
    }

    async fn run_one_step<T>(
        &self,
        buy_jpy_per_lot: f64,
        client: &mut SimulationClient,
        strategy: &T,
        market: &Market,
    ) -> MyResult<()>
    where
        T: Strategy,
    {
        client.add_market(market)?;

        let info = client.make_info(&market.pair, self.config)?;
        let now = DateTime::<Utc>::from_utc(market.recorded_at, Utc);

        match strategy.judge(&now, &info, buy_jpy_per_lot, client).await {
            Ok(_actions) => {}
            Err(_err) => {}
        };

        Ok(())
    }
}
