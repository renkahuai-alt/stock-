use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc, Weekday};
use chrono_tz::America::New_York;
use longbridge::quote::{AdjustType, Period, TradeSessions};
use longbridge::{Config, Market, QuoteContext, SimpleError};
use tokio::sync::OnceCell;

use crate::errors::{AppError, AppResult};
use crate::models::SaveCredentialsPayload;
use crate::secret_store;

#[derive(Debug, Clone)]
pub struct MarketDataTarget {
    pub target_id: String,
    pub target_type: String,
    pub provider_symbol: String,
}

#[derive(Debug, Clone)]
pub struct ProviderSecurity {
    pub target_id: String,
    pub target_type: String,
    pub display_code: String,
    pub name: String,
    pub market: Option<String>,
    pub security_type: String,
    pub currency: Option<String>,
    pub total_shares: Option<f64>,
    pub circulating_shares: Option<f64>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ProviderBar {
    pub target_id: String,
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ProviderMarketStatus {
    pub market: String,
    pub trade_date: String,
    pub market_state: String,
}

#[derive(Debug, Clone)]
pub struct ProviderQuote {
    pub target_id: String,
    pub target_type: String,
    pub provider_symbol: String,
    pub prev_close: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub updated_at: String,
    pub source_status: String,
}

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;

    fn is_available(&self) -> bool {
        true
    }

    async fn latest_trade_date(&self, market: &str) -> AppResult<String>;

    async fn fetch_static_info(
        &self,
        targets: &[MarketDataTarget],
    ) -> AppResult<Vec<ProviderSecurity>>;

    async fn fetch_daily_bars(
        &self,
        target: &MarketDataTarget,
        start_date: Option<&str>,
        end_date: &str,
    ) -> AppResult<Vec<ProviderBar>>;

    async fn market_status(&self, market: &str) -> AppResult<ProviderMarketStatus> {
        Ok(ProviderMarketStatus {
            market: market.to_string(),
            trade_date: self.latest_trade_date(market).await?,
            market_state: "closed".to_string(),
        })
    }

    async fn prewarm(&self) -> AppResult<()> {
        Ok(())
    }

    async fn fetch_realtime_quotes(
        &self,
        _targets: &[MarketDataTarget],
    ) -> AppResult<Vec<ProviderQuote>> {
        Err(AppError::Message(
            "realtime quote integration is not available yet".into(),
        ))
    }
}

pub fn provider_from_credentials() -> AppResult<Arc<dyn MarketDataProvider>> {
    match secret_store::load_credentials()? {
        Some(credentials) => Ok(Arc::new(LongbridgeMarketDataProvider::new(credentials))),
        None => Ok(Arc::new(UnavailableMarketDataProvider)),
    }
}

#[derive(Debug)]
struct UnavailableMarketDataProvider;

#[async_trait]
impl MarketDataProvider for UnavailableMarketDataProvider {
    fn provider_name(&self) -> &'static str {
        "unavailable"
    }

    fn is_available(&self) -> bool {
        false
    }

    async fn latest_trade_date(&self, _market: &str) -> AppResult<String> {
        Err(AppError::Message(
            "Longbridge credentials are not configured".into(),
        ))
    }

    async fn fetch_static_info(
        &self,
        _targets: &[MarketDataTarget],
    ) -> AppResult<Vec<ProviderSecurity>> {
        Err(AppError::Message(
            "Longbridge credentials are not configured".into(),
        ))
    }

    async fn fetch_daily_bars(
        &self,
        _target: &MarketDataTarget,
        _start_date: Option<&str>,
        _end_date: &str,
    ) -> AppResult<Vec<ProviderBar>> {
        Err(AppError::Message(
            "Longbridge credentials are not configured".into(),
        ))
    }

    async fn market_status(&self, _market: &str) -> AppResult<ProviderMarketStatus> {
        Err(AppError::Message(
            "Longbridge credentials are not configured".into(),
        ))
    }

    async fn fetch_realtime_quotes(
        &self,
        _targets: &[MarketDataTarget],
    ) -> AppResult<Vec<ProviderQuote>> {
        Err(AppError::Message(
            "Longbridge credentials are not configured".into(),
        ))
    }
}

#[derive(Clone)]
struct LongbridgeMarketDataProvider {
    credentials: SaveCredentialsPayload,
    quote_context: Arc<OnceCell<QuoteContext>>,
}

impl LongbridgeMarketDataProvider {
    fn new(credentials: SaveCredentialsPayload) -> Self {
        Self {
            credentials,
            quote_context: Arc::new(OnceCell::new()),
        }
    }

    fn normalize_provider_symbol(&self, target: &MarketDataTarget) -> String {
        if target.provider_symbol.contains('.') {
            return target.provider_symbol.clone();
        }

        if target.target_type == "index" {
            return match target.target_id.as_str() {
                "DJI" => ".DJI.US".to_string(),
                "IXIC" => ".IXIC.US".to_string(),
                "GSPC" => "SPY.US".to_string(),
                "RUT" => "IWM.US".to_string(),
                _ => target.provider_symbol.clone(),
            };
        }

        format!("{}.US", target.provider_symbol)
    }

    async fn quote_context(&self) -> AppResult<QuoteContext> {
        self.quote_context
            .get_or_try_init(|| async {
                let config = Arc::new(Config::from_apikey(
                    self.credentials.app_key.clone(),
                    self.credentials.app_secret.clone(),
                    self.credentials.access_token.clone(),
                ));
                let (quote_context, _) = QuoteContext::try_new(config)
                    .await
                    .map_err(map_longbridge_error)?;
                Ok(quote_context)
            })
            .await
            .cloned()
    }
}

#[async_trait]
impl MarketDataProvider for LongbridgeMarketDataProvider {
    fn provider_name(&self) -> &'static str {
        "longbridge"
    }

    async fn latest_trade_date(&self, market: &str) -> AppResult<String> {
        let market = parse_market(market)?;
        let quote_context = self.quote_context().await?;
        let (market_date, market_time) = market_now(market);
        let trading_day_window = quote_context
            .trading_days(
                market,
                to_time_date(market_date - Duration::days(30))?,
                to_time_date(market_date)?,
            )
            .await
            .map_err(map_longbridge_error)?;

        let trading_days = trading_day_window
            .trading_days
            .iter()
            .map(|date| from_time_date(*date))
            .collect::<AppResult<Vec<_>>>()?;
        let half_trading_days = trading_day_window
            .half_trading_days
            .iter()
            .map(|date| from_time_date(*date))
            .collect::<AppResult<Vec<_>>>()?;

        let latest = latest_completed_trading_day(
            &trading_days,
            &half_trading_days,
            market_date,
            market_time,
        )
        .ok_or_else(|| {
            AppError::Message(format!(
                "Longbridge empty data: no completed trading day found for {market}"
            ))
        })?;

        Ok(latest.format("%Y-%m-%d").to_string())
    }

    async fn fetch_static_info(
        &self,
        targets: &[MarketDataTarget],
    ) -> AppResult<Vec<ProviderSecurity>> {
        if targets.is_empty() {
            return Ok(Vec::new());
        }

        let quote_context = self.quote_context().await?;
        let mut target_by_symbol = HashMap::with_capacity(targets.len());
        let symbols: Vec<String> = targets
            .iter()
            .map(|target| {
                let provider_symbol = self.normalize_provider_symbol(target);
                target_by_symbol.insert(provider_symbol.clone(), target);
                provider_symbol
            })
            .collect();

        let now = Utc::now().to_rfc3339();
        let securities = quote_context
            .static_info(symbols)
            .await
            .map_err(map_longbridge_error)?;

        Ok(securities
            .into_iter()
            .filter_map(|security| {
                target_by_symbol
                    .get(&security.symbol)
                    .map(|target| ProviderSecurity {
                        target_id: target.target_id.clone(),
                        target_type: target.target_type.clone(),
                        display_code: display_code_for_target(target),
                        name: security_name(&security, &target.target_id),
                        market: market_code_from_symbol(&security.symbol),
                        security_type: security_type_for_target(target, &security.symbol),
                        currency: non_empty(&security.currency),
                        total_shares: positive_i64(security.total_shares),
                        circulating_shares: positive_i64(security.circulating_shares),
                        updated_at: now.clone(),
                    })
            })
            .collect())
    }

    async fn fetch_daily_bars(
        &self,
        target: &MarketDataTarget,
        start_date: Option<&str>,
        end_date: &str,
    ) -> AppResult<Vec<ProviderBar>> {
        let quote_context = self.quote_context().await?;
        let provider_symbol = self.normalize_provider_symbol(target);
        let bars = quote_context
            .history_candlesticks_by_date(
                provider_symbol.clone(),
                Period::Day,
                AdjustType::ForwardAdjust,
                start_date.map(parse_date).transpose()?,
                Some(parse_date(end_date)?),
                TradeSessions::Intraday,
            )
            .await
            .map_err(map_longbridge_error)?;

        if bars.is_empty() {
            return Err(AppError::Message(format!(
                "Longbridge empty data: no daily bars for {}",
                target.target_id
            )));
        }

        let now = Utc::now().to_rfc3339();
        bars.into_iter()
            .map(|bar| {
                Ok(ProviderBar {
                    target_id: target.target_id.clone(),
                    trade_date: bar.timestamp.date().to_string(),
                    open: decimal_to_f64(&bar.open)?,
                    high: decimal_to_f64(&bar.high)?,
                    low: decimal_to_f64(&bar.low)?,
                    close: decimal_to_f64(&bar.close)?,
                    volume: positive_i64(bar.volume),
                    source: self.provider_name().to_string(),
                    updated_at: now.clone(),
                })
            })
            .collect()
    }

    async fn market_status(&self, market: &str) -> AppResult<ProviderMarketStatus> {
        local_market_status(market)
    }

    async fn prewarm(&self) -> AppResult<()> {
        let _ = self.quote_context().await?;
        Ok(())
    }

    async fn fetch_realtime_quotes(
        &self,
        targets: &[MarketDataTarget],
    ) -> AppResult<Vec<ProviderQuote>> {
        if targets.is_empty() {
            return Ok(Vec::new());
        }

        let quote_context = self.quote_context().await?;
        let mut target_by_symbol = HashMap::with_capacity(targets.len());
        let symbols: Vec<String> = targets
            .iter()
            .map(|target| {
                let provider_symbol = self.normalize_provider_symbol(target);
                target_by_symbol.insert(provider_symbol.clone(), target);
                provider_symbol
            })
            .collect();

        let quotes = quote_context
            .quote(symbols)
            .await
            .map_err(map_longbridge_error)?;
        let updated_at = Utc::now().to_rfc3339();

        quotes
            .into_iter()
            .filter_map(|quote| {
                target_by_symbol.get(&quote.symbol).map(|target| {
                    Ok(ProviderQuote {
                        target_id: target.target_id.clone(),
                        target_type: target.target_type.clone(),
                        provider_symbol: quote.symbol.clone(),
                        prev_close: decimal_to_f64(&quote.prev_close)?,
                        open: decimal_to_f64(&quote.open)?,
                        high: decimal_to_f64(&quote.high)?,
                        low: decimal_to_f64(&quote.low)?,
                        close: decimal_to_f64(&quote.last_done)?,
                        volume: positive_i64(quote.volume),
                        updated_at: updated_at.clone(),
                        source_status: "live".to_string(),
                    })
                })
            })
            .collect()
    }
}

fn parse_market(value: &str) -> AppResult<Market> {
    match value.to_ascii_uppercase().as_str() {
        "US" => Ok(Market::US),
        "HK" => Ok(Market::HK),
        "CN" => Ok(Market::CN),
        "SG" => Ok(Market::SG),
        "CRYPTO" => Ok(Market::Crypto),
        other => Err(AppError::Message(format!("unsupported market: {other}"))),
    }
}

pub fn local_market_status(market: &str) -> AppResult<ProviderMarketStatus> {
    local_market_status_at(market, Utc::now())
}

fn local_market_status_at(market: &str, now_utc: DateTime<Utc>) -> AppResult<ProviderMarketStatus> {
    let market = parse_market(market)?;
    let (market_date, market_time) = market_now_at(market, now_utc);
    let (trade_date, market_state) = match market {
        Market::US => {
            let trade_date = if is_us_trading_day(market_date) {
                market_date
            } else {
                previous_us_trading_day(market_date)
            };
            let market_state = if is_us_market_open(market_date, market_time) {
                "open"
            } else {
                "closed"
            };
            (trade_date, market_state)
        }
        _ => (market_date, "closed"),
    };

    Ok(ProviderMarketStatus {
        market: market.to_string(),
        trade_date: trade_date.format("%Y-%m-%d").to_string(),
        market_state: market_state.to_string(),
    })
}

fn market_now(market: Market) -> (NaiveDate, NaiveTime) {
    market_now_at(market, Utc::now())
}

fn market_now_at(market: Market, now_utc: DateTime<Utc>) -> (NaiveDate, NaiveTime) {
    match market {
        Market::US => {
            let now = now_utc.with_timezone(&New_York);
            (now.date_naive(), now.time())
        }
        _ => {
            let now = now_utc;
            (now.date_naive(), now.time())
        }
    }
}

fn is_us_market_open(market_date: NaiveDate, market_time: NaiveTime) -> bool {
    is_us_trading_day(market_date)
        && market_time
            >= NaiveTime::from_hms_opt(9, 30, 0).expect("market open time should be valid")
        && market_time < us_market_close_time(market_date)
}

fn previous_us_trading_day(market_date: NaiveDate) -> NaiveDate {
    let mut candidate = market_date;
    loop {
        candidate -= Duration::days(1);
        if is_us_trading_day(candidate) {
            return candidate;
        }
    }
}

fn is_us_trading_day(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && !is_us_full_holiday(date)
}

fn us_market_close_time(date: NaiveDate) -> NaiveTime {
    if is_us_half_trading_day(date) {
        NaiveTime::from_hms_opt(13, 0, 0).expect("half trading close time should be valid")
    } else {
        NaiveTime::from_hms_opt(16, 0, 0).expect("market close time should be valid")
    }
}

fn is_us_half_trading_day(date: NaiveDate) -> bool {
    if !is_us_trading_day(date) {
        return false;
    }

    let thanksgiving = nth_weekday_of_month(date.year(), 11, Weekday::Thu, 4);
    let christmas_eve =
        NaiveDate::from_ymd_opt(date.year(), 12, 24).expect("christmas eve should be valid");
    let independence_eve =
        NaiveDate::from_ymd_opt(date.year(), 7, 3).expect("independence eve should be valid");

    date == thanksgiving + Duration::days(1)
        || (date == christmas_eve && !is_us_full_holiday(christmas_eve))
        || (date == independence_eve && !is_us_full_holiday(independence_eve))
}

fn is_us_full_holiday(date: NaiveDate) -> bool {
    let year = date.year();
    let new_year = observed_fixed_holiday(year, 1, 1);
    let next_new_year_observed = observed_fixed_holiday(year + 1, 1, 1);
    let juneteenth = observed_fixed_holiday(year, 6, 19);
    let independence_day = observed_fixed_holiday(year, 7, 4);
    let christmas = observed_fixed_holiday(year, 12, 25);

    date == new_year
        || date == next_new_year_observed
        || date == nth_weekday_of_month(year, 1, Weekday::Mon, 3)
        || date == nth_weekday_of_month(year, 2, Weekday::Mon, 3)
        || date == easter_sunday(year) - Duration::days(2)
        || date == last_weekday_of_month(year, 5, Weekday::Mon)
        || (year >= 2022 && date == juneteenth)
        || date == independence_day
        || date == nth_weekday_of_month(year, 9, Weekday::Mon, 1)
        || date == nth_weekday_of_month(year, 11, Weekday::Thu, 4)
        || date == christmas
}

fn observed_fixed_holiday(year: i32, month: u32, day: u32) -> NaiveDate {
    let holiday = NaiveDate::from_ymd_opt(year, month, day).expect("holiday should be valid");
    match holiday.weekday() {
        Weekday::Sat => holiday - Duration::days(1),
        Weekday::Sun => holiday + Duration::days(1),
        _ => holiday,
    }
}

fn nth_weekday_of_month(year: i32, month: u32, weekday: Weekday, occurrence: u8) -> NaiveDate {
    let mut date =
        NaiveDate::from_ymd_opt(year, month, 1).expect("first day of month should be valid");
    while date.weekday() != weekday {
        date += Duration::days(1);
    }

    date + Duration::days((occurrence as i64 - 1) * 7)
}

fn last_weekday_of_month(year: i32, month: u32, weekday: Weekday) -> NaiveDate {
    let mut date = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("first day of next year should be valid")
            - Duration::days(1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
            .expect("first day of next month should be valid")
            - Duration::days(1)
    };

    while date.weekday() != weekday {
        date -= Duration::days(1);
    }

    date
}

fn easter_sunday(year: i32) -> NaiveDate {
    let century = year / 100;
    let year_of_century = year % 100;
    let leap_century = century / 4;
    let remaining_century = century % 4;
    let correction = (century + 8) / 25;
    let adjusted = (century - correction + 1) / 3;
    let golden_number = year % 19;
    let epact = (19 * golden_number + century - leap_century - adjusted + 15) % 30;
    let leap_year_of_century = year_of_century / 4;
    let remaining_year = year_of_century % 4;
    let weekday_correction =
        (32 + 2 * remaining_century + 2 * leap_year_of_century - epact - remaining_year) % 7;
    let month_factor = (golden_number + 11 * epact + 22 * weekday_correction) / 451;
    let month = (epact + weekday_correction - 7 * month_factor + 114) / 31;
    let day = (epact + weekday_correction - 7 * month_factor + 114) % 31 + 1;

    NaiveDate::from_ymd_opt(year, month as u32, day as u32).expect("easter sunday should be valid")
}

fn latest_completed_trading_day(
    trading_days: &[NaiveDate],
    half_trading_days: &[NaiveDate],
    market_date: NaiveDate,
    market_time: NaiveTime,
) -> Option<NaiveDate> {
    let market_close = if half_trading_days.contains(&market_date) {
        NaiveTime::from_hms_opt(13, 0, 0).expect("half trading close time should be valid")
    } else {
        NaiveTime::from_hms_opt(16, 0, 0).expect("market close time should be valid")
    };

    if trading_days.contains(&market_date) && market_time >= market_close {
        return Some(market_date);
    }

    trading_days
        .iter()
        .copied()
        .filter(|date| *date < market_date)
        .max()
}

fn parse_date(value: &str) -> AppResult<time::Date> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|error| AppError::Message(format!("invalid date {value}: {error}")))?;
    to_time_date(date)
}

fn to_time_date(value: NaiveDate) -> AppResult<time::Date> {
    time::Date::from_calendar_date(
        value.year(),
        time::Month::try_from(value.month() as u8)
            .map_err(|error| AppError::Message(format!("invalid month: {error}")))?,
        value.day() as u8,
    )
    .map_err(|error| AppError::Message(format!("invalid calendar date: {error}")))
}

fn from_time_date(value: time::Date) -> AppResult<NaiveDate> {
    NaiveDate::from_ymd_opt(value.year(), value.month() as u32, value.day() as u32)
        .ok_or_else(|| AppError::Message(format!("invalid trading date from Longbridge: {value}")))
}

fn display_code_for_target(target: &MarketDataTarget) -> String {
    if target.target_type == "index" {
        return target.target_id.clone();
    }

    target
        .provider_symbol
        .split('.')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(target.target_id.as_str())
        .to_string()
}

fn security_name(security: &longbridge::quote::SecurityStaticInfo, fallback: &str) -> String {
    non_empty(&security.name_en)
        .or_else(|| non_empty(&security.name_hk))
        .or_else(|| non_empty(&security.name_cn))
        .unwrap_or_else(|| fallback.to_string())
}

fn market_code_from_symbol(provider_symbol: &str) -> Option<String> {
    if provider_symbol.starts_with('.') {
        return Some("US".to_string());
    }

    provider_symbol
        .rsplit_once('.')
        .map(|(_, market)| market.to_string())
}

fn security_type_for_target(target: &MarketDataTarget, provider_symbol: &str) -> String {
    if target.target_type == "index" || provider_symbol.starts_with('.') {
        "index".to_string()
    } else {
        "equity".to_string()
    }
}

fn positive_i64(value: i64) -> Option<f64> {
    (value > 0).then_some(value as f64)
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn decimal_to_f64(value: &longbridge::Decimal) -> AppResult<f64> {
    value
        .to_string()
        .parse::<f64>()
        .map_err(|error| AppError::Message(format!("invalid decimal from Longbridge: {error}")))
}

fn map_longbridge_error(error: longbridge::Error) -> AppError {
    match error.into_simple_error() {
        SimpleError::Http { status_code } => {
            let kind = match status_code {
                401 | 403 => "auth failed",
                429 => "rate limited",
                _ => "network error",
            };
            AppError::Message(format!("Longbridge {kind}: http status {status_code}"))
        }
        SimpleError::OpenApi {
            code,
            message,
            trace_id,
        } => {
            let message_lower = message.to_ascii_lowercase();
            let kind = if message_lower.contains("auth")
                || message_lower.contains("token")
                || message_lower.contains("credential")
            {
                "auth failed"
            } else if message_lower.contains("limit") || message_lower.contains("rate") {
                "rate limited"
            } else {
                "remote error"
            };
            let trace = if trace_id.is_empty() {
                String::new()
            } else {
                format!(", trace_id={trace_id}")
            };
            AppError::Message(format!(
                "Longbridge {kind}: code={code}, message={message}{trace}"
            ))
        }
        SimpleError::OAuth(message) => {
            AppError::Message(format!("Longbridge auth failed: {message}"))
        }
        SimpleError::Other(message) => {
            let kind = if message.to_ascii_lowercase().contains("timeout")
                || message.to_ascii_lowercase().contains("network")
            {
                "network error"
            } else {
                "remote error"
            };
            AppError::Message(format!("Longbridge {kind}: {message}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};

    use super::{latest_completed_trading_day, local_market_status_at};

    #[test]
    fn latest_trade_date_uses_previous_day_before_market_close() {
        let trading_days = vec![
            NaiveDate::from_ymd_opt(2026, 3, 17).expect("date should be valid"),
            NaiveDate::from_ymd_opt(2026, 3, 18).expect("date should be valid"),
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("date should be valid"),
        ];

        let latest = latest_completed_trading_day(
            &trading_days,
            &[],
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("date should be valid"),
            NaiveTime::from_hms_opt(9, 29, 0).expect("time should be valid"),
        )
        .expect("latest trading day should exist");

        assert_eq!(
            latest,
            NaiveDate::from_ymd_opt(2026, 3, 18).expect("date should be valid")
        );
    }

    #[test]
    fn latest_trade_date_uses_same_day_after_market_close() {
        let trading_days = vec![
            NaiveDate::from_ymd_opt(2026, 3, 17).expect("date should be valid"),
            NaiveDate::from_ymd_opt(2026, 3, 18).expect("date should be valid"),
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("date should be valid"),
        ];

        let latest = latest_completed_trading_day(
            &trading_days,
            &[],
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("date should be valid"),
            NaiveTime::from_hms_opt(16, 1, 0).expect("time should be valid"),
        )
        .expect("latest trading day should exist");

        assert_eq!(
            latest,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("date should be valid")
        );
    }

    #[test]
    fn latest_trade_date_uses_half_day_close_time() {
        let trading_days = vec![
            NaiveDate::from_ymd_opt(2026, 11, 26).expect("date should be valid"),
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("date should be valid"),
        ];
        let half_days = vec![NaiveDate::from_ymd_opt(2026, 11, 27).expect("date should be valid")];

        let before_close = latest_completed_trading_day(
            &trading_days,
            &half_days,
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("date should be valid"),
            NaiveTime::from_hms_opt(12, 59, 0).expect("time should be valid"),
        )
        .expect("latest trading day should exist");
        let after_close = latest_completed_trading_day(
            &trading_days,
            &half_days,
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("date should be valid"),
            NaiveTime::from_hms_opt(13, 1, 0).expect("time should be valid"),
        )
        .expect("latest trading day should exist");

        assert_eq!(
            before_close,
            NaiveDate::from_ymd_opt(2026, 11, 26).expect("date should be valid")
        );
        assert_eq!(
            after_close,
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("date should be valid")
        );
    }

    #[test]
    fn local_market_status_marks_regular_session_open() {
        let status = local_market_status_at(
            "US",
            Utc.with_ymd_and_hms(2026, 3, 19, 18, 0, 0)
                .single()
                .expect("datetime should be valid"),
        )
        .expect("market status should load");

        assert_eq!(status.trade_date, "2026-03-19");
        assert_eq!(status.market_state, "open");
    }

    #[test]
    fn local_market_status_uses_previous_trading_day_on_thanksgiving() {
        let status = local_market_status_at(
            "US",
            Utc.with_ymd_and_hms(2026, 11, 26, 20, 0, 0)
                .single()
                .expect("datetime should be valid"),
        )
        .expect("market status should load");

        assert_eq!(status.trade_date, "2026-11-25");
        assert_eq!(status.market_state, "closed");
    }
}
