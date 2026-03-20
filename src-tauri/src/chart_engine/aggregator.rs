use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};

use crate::models::{BarPoint, DailyBarRecord};

#[derive(Debug, Clone)]
pub struct WeightedSeries {
    pub weight: f64,
    pub bars: Vec<DailyBarRecord>,
}

pub fn trim_bars(bars: &[BarPoint], range: &str) -> Vec<BarPoint> {
    let count = match range {
        "1m" => 22,
        "3m" => 66,
        "6m" => 132,
        "1y" => 264,
        "3y" => 520,
        "all" => return bars.to_vec(),
        _ => return bars.to_vec(),
    };

    let start = bars.len().saturating_sub(count);
    bars[start..].to_vec()
}

pub fn aggregate_weekly(bars: &[BarPoint]) -> Vec<BarPoint> {
    let mut weeks: Vec<BarPoint> = Vec::new();
    let mut current_week: Option<(i32, u32)> = None;

    for bar in bars {
        let date = match NaiveDate::parse_from_str(&bar.time, "%Y-%m-%d") {
            Ok(value) => value,
            Err(_) => continue,
        };
        let week_key = (date.iso_week().year(), date.iso_week().week());

        match current_week {
            Some(active) if active == week_key => {
                if let Some(last) = weeks.last_mut() {
                    last.high = last.high.max(bar.high);
                    last.low = last.low.min(bar.low);
                    last.close = bar.close;
                    last.volume =
                        Some(last.volume.unwrap_or_default() + bar.volume.unwrap_or_default());
                }
            }
            _ => {
                current_week = Some(week_key);
                weeks.push(bar.clone());
            }
        }
    }

    weeks
}

pub fn build_board_bars(series: &[WeightedSeries]) -> Vec<BarPoint> {
    if series.is_empty() {
        return Vec::new();
    }

    let mut by_date: BTreeMap<String, AggregatedBar> = BTreeMap::new();

    for entry in series {
        if entry.weight <= 0.0 || entry.bars.is_empty() {
            continue;
        }

        let base_close = entry
            .bars
            .first()
            .map(|bar| bar.close)
            .filter(|value| *value > 0.0)
            .unwrap_or(1.0);

        for bar in &entry.bars {
            let bucket = by_date.entry(bar.trade_date.clone()).or_default();
            bucket.weight += entry.weight;
            bucket.open += entry.weight * (bar.open / base_close) * 100.0;
            bucket.high += entry.weight * (bar.high / base_close) * 100.0;
            bucket.low += entry.weight * (bar.low / base_close) * 100.0;
            bucket.close += entry.weight * (bar.close / base_close) * 100.0;
            bucket.volume += entry.weight * bar.volume.unwrap_or_default();
        }
    }

    by_date
        .into_iter()
        .filter_map(|(date, aggregate)| {
            if aggregate.weight <= 0.0 {
                return None;
            }

            Some(BarPoint {
                time: date,
                open: round_to(aggregate.open / aggregate.weight),
                high: round_to(aggregate.high / aggregate.weight),
                low: round_to(aggregate.low / aggregate.weight),
                close: round_to(aggregate.close / aggregate.weight),
                volume: Some(round_to(aggregate.volume)),
            })
        })
        .collect()
}

#[derive(Default)]
struct AggregatedBar {
    weight: f64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

fn round_to(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
