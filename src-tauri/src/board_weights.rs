use std::collections::HashMap;

use crate::errors::{AppError, AppResult};
use crate::models::SymbolRecord;

const BOARD_VALUE_MODE: &str = "synthetic_board_points";
const MARKET_CAP_WEIGHT_SNAPSHOT: &str = "previous_close_x_shares";

pub fn resolve_snapshot_weights(
    members: &[String],
    symbol_rows: &[SymbolRecord],
    latest_closes: &HashMap<String, f64>,
    algorithm: &str,
) -> AppResult<Vec<(String, f64)>> {
    match algorithm {
        "equal_weight_v1" => resolve_equal_weights(members),
        "market_cap_weight_v1" => {
            resolve_market_cap_snapshot_weights(members, symbol_rows, latest_closes)
        }
        other => Err(AppError::Message(format!(
            "unsupported boardAlgorithm: {other}"
        ))),
    }
}

pub fn renormalize_weights(
    weights: &[(String, f64)],
    included_members: &[String],
) -> AppResult<Vec<(String, f64)>> {
    let included = included_members
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let selected = weights
        .iter()
        .filter(|(member, weight)| included.contains(member) && *weight > 0.0)
        .cloned()
        .collect::<Vec<_>>();

    let total = selected.iter().map(|(_, weight)| *weight).sum::<f64>();
    if selected.is_empty() || total <= 0.0 {
        return Ok(Vec::new());
    }

    Ok(selected
        .into_iter()
        .map(|(member, weight)| (member, weight / total))
        .collect())
}

pub fn board_value_mode() -> Option<String> {
    Some(BOARD_VALUE_MODE.to_string())
}

pub fn board_weight_snapshot(
    algorithm: &str,
    snapshot_trade_date: Option<&str>,
) -> (Option<String>, Option<String>) {
    match algorithm {
        "market_cap_weight_v1" => (
            Some(MARKET_CAP_WEIGHT_SNAPSHOT.to_string()),
            snapshot_trade_date.map(str::to_string),
        ),
        _ => (None, None),
    }
}

fn resolve_equal_weights(members: &[String]) -> AppResult<Vec<(String, f64)>> {
    if members.is_empty() {
        return Ok(Vec::new());
    }

    let weight = 1.0 / members.len() as f64;
    Ok(members
        .iter()
        .map(|member| (member.clone(), weight))
        .collect())
}

fn resolve_market_cap_snapshot_weights(
    members: &[String],
    symbol_rows: &[SymbolRecord],
    latest_closes: &HashMap<String, f64>,
) -> AppResult<Vec<(String, f64)>> {
    let mut pairs = Vec::with_capacity(members.len());
    let mut total = 0.0;

    for member in members {
        let row = symbol_rows
            .iter()
            .find(|record| record.target_id == *member)
            .ok_or_else(|| AppError::Message(format!("symbol not found: {member}")))?;
        let shares = row.circulating_shares.or(row.total_shares).ok_or_else(|| {
            AppError::Message(format!("market cap weight requires shares for {member}"))
        })?;
        let latest_close = latest_closes.get(member).copied().ok_or_else(|| {
            AppError::Message(format!(
                "market cap weight requires latest local close for {member}"
            ))
        })?;
        let estimated_market_cap = shares * latest_close;
        total += estimated_market_cap;
        pairs.push((member.clone(), estimated_market_cap));
    }

    if total <= 0.0 {
        return Err(AppError::Message(
            "market cap weight requires positive total snapshot market cap".to_string(),
        ));
    }

    Ok(pairs
        .into_iter()
        .map(|(member, estimated_market_cap)| (member, estimated_market_cap / total))
        .collect())
}
