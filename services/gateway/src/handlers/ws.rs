use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::Response,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use tokio::time::{interval, Duration, MissedTickBehavior};

const MAX_HISTORY: usize = 10_000;

#[derive(Debug, Deserialize)]
pub struct WsConnectQuery {
    token: Option<String>,
}

#[derive(Debug, Clone)]
struct Subscription {
    channel: String,
    params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct EventMetadata {
    version: String,
    correlation_id: String,
    causation_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct EventEnvelope {
    event_id: String,
    event_type: String,
    sequence: String,
    timestamp: String,
    source: String,
    payload: Value,
    metadata: EventMetadata,
}

#[derive(Debug, Serialize)]
struct ConnectedMessage {
    r#type: &'static str,
    session_id: String,
}

#[derive(Debug, Serialize)]
struct PingMessage {
    r#type: &'static str,
}

#[derive(Debug, Serialize)]
struct SubscriptionAck {
    r#type: &'static str,
    channel: String,
    params: BTreeMap<String, String>,
    snapshot_seq: u64,
}

#[derive(Debug, Serialize)]
struct UnsubscribedMessage {
    r#type: &'static str,
    channel: String,
    params: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct SnapshotSinceResponse {
    r#type: &'static str,
    channel: String,
    from_seq: u64,
    to_seq: u64,
    events: Vec<EventEnvelope>,
}

#[derive(Debug, Serialize)]
struct ErrorMessage {
    r#type: &'static str,
    code: &'static str,
    message: String,
}

#[derive(Debug, Default)]
struct SessionState {
    sequence: u64,
    tick: u64,
    history: VecDeque<EventEnvelope>,
    subscriptions: HashMap<String, Subscription>,
    symbols: HashMap<String, SymbolState>,
}

#[derive(Debug, Clone)]
struct SymbolState {
    last_price_cents: i64,
    volume_bps: i64,
    high_cents: i64,
    low_cents: i64,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsConnectQuery>,
) -> Result<Response, AppError> {
    if query.token.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::Unauthorized("Missing websocket token".into()));
    }

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let session_id = format!("sess-{}", state.record_ws_connection());
    let mut session = SessionState::default();

    if send_json(
        &mut socket,
        &ConnectedMessage {
            r#type: "connected",
            session_id,
        },
    )
    .await
    .is_err()
    {
        return;
    }

    let mut ping_timer = interval(Duration::from_secs(15));
    ping_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut publish_timer = interval(Duration::from_millis(400));
    publish_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ping_timer.tick() => {
                if send_json(&mut socket, &PingMessage { r#type: "ping" }).await.is_err() {
                    break;
                }
            }
            _ = publish_timer.tick() => {
                if publish_subscriptions(&mut socket, &state, &mut session).await.is_err() {
                    break;
                }
            }
            msg = socket.next() => {
                let Some(msg) = msg else {
                    break;
                };

                let Ok(msg) = msg else {
                    break;
                };

                if handle_client_message(msg, &mut socket, &state, &mut session).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn handle_client_message(
    msg: Message,
    socket: &mut WebSocket,
    state: &AppState,
    session: &mut SessionState,
) -> Result<(), AppError> {
    match msg {
        Message::Text(text) => {
            let value = match serde_json::from_str::<Value>(text.as_str()) {
                Ok(value) => value,
                Err(_) => return Ok(()),
            };

            if value.get("type").and_then(Value::as_str) == Some("pong") {
                return Ok(());
            }

            match value.get("action").and_then(Value::as_str) {
                Some("subscribe") => handle_subscribe(value, socket, state, session).await,
                Some("unsubscribe") => handle_unsubscribe(value, socket, session).await,
                Some("snapshot_since") => handle_snapshot_since(value, socket, session).await,
                Some(_) => {
                    send_json(
                        socket,
                        &ErrorMessage {
                            r#type: "error",
                            code: "INVALID_ACTION",
                            message: "Unsupported websocket action".to_string(),
                        },
                    )
                    .await
                    .map_err(map_ws_err)
                }
                None => Ok(()),
            }
        }
        Message::Close(_) => Err(AppError::BadRequest("socket closed".into())),
        _ => Ok(()),
    }
}

async fn handle_subscribe(
    value: Value,
    socket: &mut WebSocket,
    state: &AppState,
    session: &mut SessionState,
) -> Result<(), AppError> {
    let Some(channel) = value.get("channel").and_then(Value::as_str) else {
        return Ok(());
    };
    if !matches!(channel, "market_data" | "trades" | "account") {
        return send_json(
            socket,
            &ErrorMessage {
                r#type: "error",
                code: "INVALID_CHANNEL",
                message: format!("Channel '{}' does not exist", channel),
            },
        )
        .await
        .map_err(map_ws_err);
    }

    let params = extract_params(value.get("params"));
    let key = subscription_key(channel, &params);
    session.subscriptions.insert(
        key,
        Subscription {
            channel: channel.to_string(),
            params: params.clone(),
        },
    );

    send_json(
        socket,
        &SubscriptionAck {
            r#type: "subscribed",
            channel: channel.to_string(),
            params: params.clone(),
            snapshot_seq: session.sequence,
        },
    )
    .await
    .map_err(map_ws_err)?;

    match channel {
        "market_data" => {
            if let Some(symbol) = params.get("symbol") {
                let snapshot = next_orderbook_snapshot(session, symbol);
                send_event(socket, session, snapshot).await?;

                let ticker = next_ticker_delta(session, symbol);
                send_event(socket, session, ticker).await?;
            }
        }
        "account" => {
            if let Some(account_id) = params.get("account_id") {
                let snapshot = next_account_snapshot(session, state, account_id).await;
                send_event(socket, session, snapshot).await?;
            }
        }
        "trades" => {}
        _ => {}
    }

    Ok(())
}

async fn handle_unsubscribe(
    value: Value,
    socket: &mut WebSocket,
    session: &mut SessionState,
) -> Result<(), AppError> {
    let Some(channel) = value.get("channel").and_then(Value::as_str) else {
        return Ok(());
    };
    let params = extract_params(value.get("params"));
    let key = subscription_key(channel, &params);
    session.subscriptions.remove(&key);

    send_json(
        socket,
        &UnsubscribedMessage {
            r#type: "unsubscribed",
            channel: channel.to_string(),
            params,
        },
    )
    .await
    .map_err(map_ws_err)
}

async fn handle_snapshot_since(
    value: Value,
    socket: &mut WebSocket,
    session: &mut SessionState,
) -> Result<(), AppError> {
    let Some(channel) = value.get("channel").and_then(Value::as_str) else {
        return Ok(());
    };
    let params = extract_params(value.get("params"));
    let from_seq = params
        .get("last_seq")
        .and_then(|seq| seq.parse::<u64>().ok())
        .unwrap_or(0);

    let events = session
        .history
        .iter()
        .filter(|event| event.source == channel)
        .filter(|event| event_matches(event, channel, &params))
        .filter(|event| event.sequence.parse::<u64>().ok().unwrap_or(0) > from_seq)
        .cloned()
        .collect::<Vec<_>>();

    send_json(
        socket,
        &SnapshotSinceResponse {
            r#type: "snapshot_since_response",
            channel: channel.to_string(),
            from_seq,
            to_seq: session.sequence,
            events,
        },
    )
    .await
    .map_err(map_ws_err)
}

async fn publish_subscriptions(
    socket: &mut WebSocket,
    state: &AppState,
    session: &mut SessionState,
) -> Result<(), AppError> {
    if session.subscriptions.is_empty() {
        return Ok(());
    }

    session.tick += 1;
    let subscriptions = session.subscriptions.values().cloned().collect::<Vec<_>>();

    for subscription in subscriptions {
        match subscription.channel.as_str() {
            "market_data" => {
                if let Some(symbol) = subscription.params.get("symbol") {
                    let event = if session.tick % 2 == 0 {
                        next_ticker_delta(session, symbol)
                    } else {
                        next_orderbook_delta(session, symbol)
                    };
                    send_event(socket, session, event).await?;
                }
            }
            "trades" => {
                if let Some(symbol) = subscription.params.get("symbol") {
                    let event = next_trade_delta(session, symbol);
                    send_event(socket, session, event).await?;
                }
            }
            "account" => {
                if session.tick % 4 == 0 {
                    if let Some(account_id) = subscription.params.get("account_id") {
                        let event = next_account_snapshot(session, state, account_id).await;
                        send_event(socket, session, event).await?;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

async fn next_account_snapshot(
    session: &mut SessionState,
    state: &AppState,
    account_id: &str,
) -> EventEnvelope {
    let snapshot = state.get_mock_account_stream_snapshot(account_id).await;
    next_event(
        session,
        "account",
        "snapshot",
        json!({
            "account_id": snapshot.account_id,
            "balances": snapshot.balances,
            "orders": snapshot.orders,
        }),
    )
}

fn next_orderbook_snapshot(session: &mut SessionState, symbol: &str) -> EventEnvelope {
    let market = symbol_state(session, symbol);
    let bids = (0..15)
        .map(|idx| {
            [
                cents_to_price(market.last_price_cents - (idx as i64 * 25)),
                bps_to_quantity(8_000 - (idx as i64 * 175)),
            ]
        })
        .collect::<Vec<_>>();
    let asks = (0..15)
        .map(|idx| {
            [
                cents_to_price(market.last_price_cents + 25 + (idx as i64 * 25)),
                bps_to_quantity(8_200 - (idx as i64 * 160)),
            ]
        })
        .collect::<Vec<_>>();

    next_event(
        session,
        "market_data",
        "snapshot",
        json!({
            "symbol": symbol,
            "bids": bids,
            "asks": asks,
        }),
    )
}

fn next_orderbook_delta(session: &mut SessionState, symbol: &str) -> EventEnvelope {
    let tick = session.tick as i64;
    let market = symbol_state(session, symbol);
    let side = if tick % 2 == 0 { "bids" } else { "asks" };
    let price_offset = ((tick % 7) - 3) * 18;
    let price = if side == "bids" {
        market.last_price_cents - 50 + price_offset
    } else {
        market.last_price_cents + 50 + price_offset
    };
    let quantity = if tick % 9 == 0 {
        "0".to_string()
    } else {
        bps_to_quantity(2_500 + ((tick * 137) % 3_500))
    };

    next_event(
        session,
        "market_data",
        "delta",
        json!({
            "symbol": symbol,
            side: [[cents_to_price(price), quantity]],
        }),
    )
}

fn next_ticker_delta(session: &mut SessionState, symbol: &str) -> EventEnvelope {
    let tick = session.tick as i64;
    let market = symbol_state(session, symbol);
    let movement = ((tick * 11 + symbol.len() as i64) % 9) - 4;
    market.last_price_cents += movement * 5;
    market.high_cents = market.high_cents.max(market.last_price_cents);
    market.low_cents = market.low_cents.min(market.last_price_cents);
    market.volume_bps += 450 + (tick % 13) * 10;
    let last_price = market.last_price_cents;
    let high = market.high_cents;
    let low = market.low_cents;
    let volume = market.volume_bps;
    let mark_price = market.last_price_cents + 8;

    next_event(
        session,
        "market_data",
        "delta",
        json!({
            "symbol": symbol,
            "last_price": cents_to_price(last_price),
            "volume_24h": bps_to_quantity(volume),
            "high_24h": cents_to_price(high),
            "low_24h": cents_to_price(low),
            "mark_price": cents_to_price(mark_price),
        }),
    )
}

fn next_trade_delta(session: &mut SessionState, symbol: &str) -> EventEnvelope {
    let tick = session.tick as i64;
    let market = symbol_state(session, symbol);
    let trade_price = market.last_price_cents + ((tick % 5) - 2) * 4;
    let quantity = bps_to_quantity(900 + ((tick * 97) % 1_800));
    let side = if tick % 2 == 0 { "BUY" } else { "SELL" };

    next_event(
        session,
        "trades",
        "delta",
        json!({
            "symbol": symbol,
            "price": cents_to_price(trade_price),
            "quantity": quantity,
            "side": side,
        }),
    )
}

fn next_event(
    session: &mut SessionState,
    source: &str,
    event_type: &str,
    payload: Value,
) -> EventEnvelope {
    session.sequence += 1;

    EventEnvelope {
        event_id: format!("evt-{}", session.sequence),
        event_type: event_type.to_string(),
        sequence: session.sequence.to_string(),
        timestamp: unix_timestamp_nanos().to_string(),
        source: source.to_string(),
        payload,
        metadata: EventMetadata {
            version: "1.0".to_string(),
            correlation_id: String::new(),
            causation_id: String::new(),
        },
    }
}

async fn send_event(
    socket: &mut WebSocket,
    session: &mut SessionState,
    event: EventEnvelope,
) -> Result<(), AppError> {
    session.history.push_back(event.clone());
    if session.history.len() > MAX_HISTORY {
        session.history.pop_front();
    }
    send_json(socket, &event).await.map_err(map_ws_err)
}

fn symbol_state<'a>(session: &'a mut SessionState, symbol: &str) -> &'a mut SymbolState {
    session
        .symbols
        .entry(symbol.to_string())
        .or_insert_with(|| default_symbol_state(symbol))
}

fn default_symbol_state(symbol: &str) -> SymbolState {
    let seed = symbol.bytes().map(i64::from).sum::<i64>();
    let base = 45_000_00 + seed * 75;
    SymbolState {
        last_price_cents: base,
        volume_bps: 120_000,
        high_cents: base + 250,
        low_cents: base - 250,
    }
}

fn subscription_key(channel: &str, params: &BTreeMap<String, String>) -> String {
    let suffix = params
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    format!("{channel}::{suffix}")
}

fn extract_params(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|params| {
            params
                .iter()
                .map(|(key, value)| {
                    let rendered = value
                        .as_str()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| value.to_string().trim_matches('"').to_string());
                    (key.clone(), rendered)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn event_matches(event: &EventEnvelope, channel: &str, params: &BTreeMap<String, String>) -> bool {
    match channel {
        "market_data" | "trades" => {
            let event_symbol = event
                .payload
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or_default();
            params
                .get("symbol")
                .map(|symbol| symbol == event_symbol)
                .unwrap_or(false)
        }
        "account" => {
            let event_account = event
                .payload
                .get("account_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            params
                .get("account_id")
                .map(|account_id| account_id == event_account)
                .unwrap_or(false)
        }
        _ => false,
    }
}

fn cents_to_price(cents: i64) -> String {
    let whole = cents / 100;
    let fractional = cents.abs() % 100;
    format!("{whole}.{fractional:02}")
}

fn bps_to_quantity(bps: i64) -> String {
    let whole = bps / 10_000;
    let fractional = bps.abs() % 10_000;
    format!("{whole}.{fractional:04}")
}

fn unix_timestamp_nanos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as i64)
        .unwrap_or(0)
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, message: &T) -> Result<(), axum::Error> {
    let payload = serde_json::to_string(message).expect("serializable websocket payload");
    socket.send(Message::Text(Utf8Bytes::from(payload))).await
}

fn map_ws_err(err: axum::Error) -> AppError {
    AppError::InternalError(anyhow::anyhow!(err))
}
