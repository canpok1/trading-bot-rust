use crate::coincheck::model::Balance;
use crate::coincheck::model::OpenOrder;
use crate::coincheck::model::OrderBook;
use chrono::{DateTime, Duration, Timelike, Utc};

pub fn has_unused_coin(balance: &Balance, amount_per_lot: f64) -> (bool, String) {
    if balance.amount < amount_per_lot {
        (
            true,
            format!(
                "has not unused coin, coin:{:.3} < amount_per_lot:{:.3}",
                balance.amount, amount_per_lot,
            ),
        )
    } else {
        (
            false,
            format!(
                "has unused coin, coin:{:.3} >= amount_per_lot:{:.3}",
                balance.amount, amount_per_lot,
            ),
        )
    }
}

pub fn is_notification_timing(now: &DateTime<Utc>) -> (bool, String) {
    let minute = now.minute();
    if minute % 5 == 0 {
        (
            true,
            format!("it is notification timing now, minute:{} % 5 == 0", minute),
        )
    } else {
        (
            false,
            format!(
                "it is not notification timing now, minute:{} % 5 != 0",
                minute
            ),
        )
    }
}

pub fn should_loss_cut(
    sell_rate: f64,
    open_order: &OpenOrder,
    loss_cut_rate_ratio: f64,
) -> (bool, String) {
    let lower = open_order.rate * loss_cut_rate_ratio;
    if sell_rate < lower {
        (
            true,
            format!(
                "should loss cut, sell_rate:{:.3} < lower:{:.3}",
                sell_rate, lower,
            ),
        )
    } else {
        (
            false,
            format!(
                "should not loss cut, sell_rate:{:.3} >= lower:{:.3}",
                sell_rate, lower,
            ),
        )
    }
}

pub fn should_avg_down(
    now: &DateTime<Utc>,
    buy_rate: f64,
    open_order: &OpenOrder,
    avg_down_rate_ratio: f64,
    hold_limit_minutes: i64,
) -> (bool, String) {
    let lower = open_order.rate * avg_down_rate_ratio;
    let holding_term = *now - open_order.created_at.with_timezone(&Utc);
    let holding_limit = Duration::minutes(hold_limit_minutes);
    if buy_rate < lower {
        (
            true,
            format!(
                "should avg down, buy_rate{:.3} < lower:{:.3}",
                buy_rate, lower,
            ),
        )
    } else if holding_term > holding_limit {
        (
            true,
            format!(
                "should avg down, holding_term:{} > limit:{}",
                holding_term, holding_limit
            ),
        )
    } else {
        (
            false,
            format!(
                "should not avg down, buy_rate{:.3} >= lower:{:.3}, holding_term:{} <= limit:{}",
                buy_rate, lower, holding_term, holding_limit
            ),
        )
    }
}

pub fn calc_avg_down_buy_amount(buy_jpy_per_lot: f64, open_order: &OpenOrder) -> (f64, String) {
    let used_jpy = open_order.rate * open_order.pending_amount;
    let mut used_jpy_tmp = 0.0;
    let mut lot = 1.0;
    while used_jpy_tmp <= used_jpy * 0.8 {
        used_jpy_tmp += lot * buy_jpy_per_lot;
        // 1, 2, 4, 8, 16, 32 ... の割合でナンピンする
        lot *= 2.0;
    }
    (
        lot * buy_jpy_per_lot,
        format! {"lot: {}, buy_jpy: {}, used_jpy: {}", lot, buy_jpy_per_lot, used_jpy},
    )
}

// 下降トレンドか判断
// 移動平均の短期が長期より下なら下降トレンドと判断
pub fn is_down_trend(wma_short: f64, wma_long: f64) -> (bool, String) {
    if wma_short < wma_long {
        (
            true,
            format!(
                "is down trend, wma_short:{:.3} < wma_long:{:.3}",
                wma_short, wma_long,
            ),
        )
    } else {
        (
            false,
            format!(
                "is not down trend, wma_short:{:.3} >= wma_long:{:.3}",
                wma_short, wma_long,
            ),
        )
    }
}

pub fn has_near_rate_order(
    sell_rate: f64,
    open_orders: &Vec<OpenOrder>,
    entry_skip_rate_ratio: f64,
) -> (bool, String) {
    if open_orders.is_empty() {
        return (
            false,
            "has not near rate order, open_orders is empty".to_string(),
        );
    }

    let mut min_rate = 0.0;
    for (i, o) in open_orders.iter().enumerate() {
        if i == 0 || min_rate > o.rate {
            min_rate = o.rate;
        }
    }

    if sell_rate > min_rate * entry_skip_rate_ratio {
        (
            true,
            format!(
                "has near rate order, sell_rate:{:.3} > (min_rate:{:.3} * {:.3})",
                sell_rate, min_rate, entry_skip_rate_ratio
            ),
        )
    } else {
        (
            false,
            format!(
                "has not near rate order, sell_rate:{:.3} <= (min_rate:{:.3} * {:.3})",
                sell_rate, min_rate, entry_skip_rate_ratio
            ),
        )
    }
}

pub fn is_trade_frequency_enough(
    trade_frequency_ratio: f64,
    required_trade_frequency_ratio: f64,
) -> (bool, String) {
    if trade_frequency_ratio < required_trade_frequency_ratio {
        (
            false,
            format!(
                "trade frequency is too low, frequency:{:.3} < required:{:.3}",
                trade_frequency_ratio, required_trade_frequency_ratio
            ),
        )
    } else {
        (
            true,
            format!(
                "trade frequency is enough, frequency:{:.3} >= required:{:.3}",
                trade_frequency_ratio, required_trade_frequency_ratio
            ),
        )
    }
}

pub fn is_over_sell(
    short_volume_sell_total: f64,
    short_volume_buy_total: f64,
    long_volume_sell_total: f64,
    over_sell_volume_ratio: f64,
) -> (bool, String) {
    let volume_diff = short_volume_sell_total - short_volume_buy_total;
    let over_sell_volume_border = long_volume_sell_total * over_sell_volume_ratio;

    if volume_diff >= over_sell_volume_border {
        (
            true,
            format!(
                "over sell, volume_diff:{:.3} >= border:{:.3}",
                volume_diff, over_sell_volume_border
            ),
        )
    } else {
        (
            false,
            format!(
                "not over sell, volume_diff:{:.3} < border:{:.3}",
                volume_diff, over_sell_volume_border
            ),
        )
    }
}

pub fn is_board_heavy(
    order_sell_rate: f64,
    order_book_asks: &Vec<OrderBook>,
    short_volume_sell_total: f64,
    order_books_size_ratio: f64,
) -> (bool, String) {
    let mut ask_total = 0.0;
    for ask in order_book_asks.iter() {
        if ask.rate < order_sell_rate {
            ask_total += ask.amount;
        }
    }
    let ask_total_upper = short_volume_sell_total * order_books_size_ratio;
    if ask_total > ask_total_upper {
        (
            true,
            format!(
                "board is too heavy, ask_total:{:.3} > border:{:.3}",
                ask_total, ask_total_upper
            ),
        )
    } else {
        (
            false,
            format!(
                "board is not heavy, ask_total:{:.3} <= border:{:.3}",
                ask_total, ask_total_upper
            ),
        )
    }
}

pub fn estimate_sell_rate(buy_rate: f64, buy_jpy_per_lot: f64, profit_ratio: f64) -> f64 {
    let amount = buy_jpy_per_lot / buy_rate;
    let profit = buy_jpy_per_lot * profit_ratio;
    (buy_jpy_per_lot + profit) / amount
}

pub fn is_on_line(
    sell_rate: f64,
    lines: &Vec<f64>,
    line_width_ratio_upper: f64,
    line_width_ratio_lower: f64,
) -> (bool, String) {
    let width_upper = sell_rate * line_width_ratio_upper;
    let width_lower = sell_rate * line_width_ratio_lower;
    let upper = lines.last().unwrap() + width_upper;
    let lower = lines.last().unwrap() - width_lower;
    let result = sell_rate >= lower && sell_rate <= upper;
    let message = if result {
        format!(
            "sell rate:{} is on line:{}",
            format!("{:.3}", sell_rate),
            format!("{:.3}...{:.3}", lower, upper)
        )
    } else {
        format!(
            "sell rate:{} is not on line:{}",
            format!("{:.3}", sell_rate),
            format!("{:.3}...{:.3}", lower, upper)
        )
    };
    (result, message)
}

pub fn is_rebounded(
    sell_rate: f64,
    sell_rate_histories: &Vec<f64>,
    lines: &Vec<f64>,
    line_width_ratio_upper: f64,
    line_width_ratio_lower: f64,
    rebound_check_period: usize,
) -> (bool, String) {
    let width_upper = sell_rate * line_width_ratio_upper;
    let width_lower = sell_rate * line_width_ratio_lower;
    let rebounded = is_upper_rebound(
        sell_rate_histories,
        lines,
        width_upper,
        width_lower,
        rebound_check_period,
    );
    if rebounded {
        (true, "is_rebound: true".to_string())
    } else {
        (false, "is_rebound: false".to_string())
    }
}

pub fn is_upper_rebound(
    rates: &Vec<f64>,
    lines: &Vec<f64>,
    line_width_upper: f64,
    line_width_lower: f64,
    period: usize,
) -> bool {
    let history_size = rates.len();
    if history_size < period {
        return false;
    }

    let end_idx = history_size - 1;
    let begin_idx = end_idx - period;
    for idx in (begin_idx..=end_idx).rev() {
        let rate1 = rates.iter().nth(idx - 2);
        let rate2 = rates.iter().nth(idx - 1);
        let rate3 = rates.iter().nth(idx);
        if rate1.is_none() || rate2.is_none() || rate3.is_none() {
            return false;
        }
        let line1 = lines.iter().nth(idx - 2);
        let line2 = lines.iter().nth(idx - 1);
        let line3 = lines.iter().nth(idx);
        if line1.is_none() || line2.is_none() || line3.is_none() {
            return false;
        }

        let rate1 = *rate1.unwrap();
        let rate2 = *rate2.unwrap();
        let rate3 = *rate3.unwrap();
        let line1 = *line1.unwrap();
        let line2 = *line2.unwrap();
        let line3 = *line3.unwrap();

        // rate1,rate2,rate3 のいずれかがラインを下回ったらチェック打ち切り
        if rate1 < (line1 - line_width_lower)
            || rate2 < (line2 - line_width_lower)
            || rate3 < (line3 - line_width_lower)
        {
            return false;
        }

        // rate1,rate2,rate3 が v字 になってないならスキップ
        if !(rate1 >= rate2 && rate2 < rate3) {
            continue;
        }

        // v字の底がラインから離れすぎていたらスキップ
        if rate2 > line2 + line_width_upper {
            continue;
        }

        return true;
    }
    false
}

pub fn calc_slope(line: &Vec<f64>) -> f64 {
    line[1] - line[0]
}
