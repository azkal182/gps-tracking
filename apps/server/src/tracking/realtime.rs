use super::super::*;
use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyTicketRequest {}

#[derive(Serialize)]
struct TicketResponse {
    ticket: String,
    expires_at: String,
}

#[derive(Deserialize)]
pub(crate) struct TicketQuery {
    ticket: Option<String>,
}

pub(crate) async fn create_realtime_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_): Json<EmptyTicketRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.clone().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let pool = state
        .tracking_pool
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let expiry = Instant::now() + Duration::from_secs(60);
    let (tenant, principal) = if let Some(value) = headers.get(header::AUTHORIZATION) {
        if headers.contains_key("x-organization-id") {
            return Err(StatusCode::BAD_REQUEST);
        }
        let value = value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        let token = value
            .strip_prefix("Bearer ")
            .filter(|v| v.starts_with("gps_app_"))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let token_hash = digest(token);
        let identity = tracking_storage::lookup_application_token(&pool, &token_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if !identity
            .scopes
            .iter()
            .any(|scope| scope == "tracking:subscribe")
        {
            return Err(StatusCode::FORBIDDEN);
        }
        (
            identity.tenant_id.clone(),
            RealtimePrincipal::Application {
                token_id: identity.token_id,
                owner_user_id: identity.owner_user_id,
                token_hash: token_hash.to_vec(),
            },
        )
    } else {
        let user = session_user_csrf(&auth, &headers).await?;
        let tenant = require_tenant_member(&auth, &user, &headers).await?;
        (
            tenant,
            RealtimePrincipal::Session {
                session_id: user.session_id,
                user_id: user.user_id,
            },
        )
    };
    let expires_at: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()+interval '60 seconds') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let secret = format!("gps_ws_{}", random_secret());
    let ticket_hash = URL_SAFE_NO_PAD.encode(digest(&secret));
    let mut tickets = state
        .realtime_tickets
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tickets.retain(|_, ticket| ticket.expires_at > Instant::now());
    if tickets.len() >= 10_000 {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    tickets.insert(
        ticket_hash,
        RealtimeTicket {
            tenant_id: tenant,
            expires_at: expiry,
            principal,
        },
    );
    Ok(Json(TicketResponse {
        ticket: secret,
        expires_at,
    })
    .into_response())
}

pub(crate) async fn tracking_websocket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TicketQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    let auth = state.auth.clone().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let ticket = if let Some(secret) = query.ticket.as_deref() {
        let ticket_hash = URL_SAFE_NO_PAD.encode(digest(secret));
        state
            .realtime_tickets
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .remove(&ticket_hash)
            .ok_or(StatusCode::UNAUTHORIZED)?
    } else {
        let value = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let token = value
            .strip_prefix("Bearer ")
            .filter(|value| value.starts_with("gps_app_"))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let pool = state
            .tracking_pool
            .clone()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        let token_hash = digest(token);
        let identity = tracking_storage::lookup_application_token(&pool, &token_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if !identity
            .scopes
            .iter()
            .any(|scope| scope == "tracking:subscribe")
        {
            return Err(StatusCode::FORBIDDEN);
        }
        RealtimeTicket {
            tenant_id: identity.tenant_id,
            expires_at: Instant::now() + Duration::from_secs(60),
            principal: RealtimePrincipal::Application {
                token_id: identity.token_id,
                owner_user_id: identity.owner_user_id,
                token_hash: token_hash.to_vec(),
            },
        }
    };
    let is_browser = matches!(&ticket.principal, RealtimePrincipal::Session { .. });
    if (is_browser || headers.contains_key(header::ORIGIN)) && !same_origin(&headers, &auth.origin)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if ticket.expires_at <= Instant::now() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !principal_is_current(&auth.pool, state.tracking_pool.as_ref(), &ticket).await {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let owner = principal_owner(&ticket.principal).to_owned();
    {
        let mut counts = state
            .realtime_connections
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if counts.get(&owner).copied().unwrap_or(0) >= 5 {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        *counts.entry(owner.clone()).or_default() += 1;
    }
    let connection_id = uuid::Uuid::new_v4().to_string();
    let subscriptions = Arc::new(Mutex::new(None));
    let (sender, receiver) = tokio::sync::mpsc::channel(256);
    state
        .realtime_clients
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .insert(
            connection_id.clone(),
            RealtimeConnection {
                tenant_id: ticket.tenant_id.clone(),
                subscriptions: subscriptions.clone(),
                sender,
            },
        );
    Ok(ws.on_upgrade(move |socket| {
        serve_socket(
            socket,
            state,
            ticket,
            owner,
            connection_id,
            subscriptions,
            receiver,
        )
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subscribe {
    #[serde(rename = "type")]
    kind: String,
    request_id: String,
    target_ids: Vec<String>,
    device_ids: Vec<String>,
}

#[derive(Serialize)]
struct Subscribed<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    request_id: &'a str,
    target_ids: &'a [String],
    device_ids: &'a [String],
}

async fn serve_socket(
    mut socket: WebSocket,
    state: AppState,
    ticket: RealtimeTicket,
    owner: String,
    connection_id: String,
    subscription_state: SharedRealtimeSubscriptions,
    mut event_receiver: tokio::sync::mpsc::Receiver<RealtimeEvent>,
) {
    let _connection_guard = ConnectionGuard {
        counts: state.realtime_connections.clone(),
        clients: state.realtime_clients.clone(),
        owner,
        connection_id,
    };
    let Some(auth) = state.auth.clone() else {
        return;
    };
    let Some(pool) = state.tracking_pool.clone() else {
        return;
    };
    let mut auth_check = tokio::time::interval(Duration::from_secs(5));
    auth_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let message = tokio::select! {
        _ = auth_check.tick() => {
            if !principal_is_current(&auth.pool, Some(&pool), &ticket).await { return; }
            if !subscriptions_current(&pool, &ticket, &subscription_state).await { return; }
            continue;
            }
            event = event_receiver.recv() => match event {
                Some(event) => {
                    if event.tenant_id != ticket.tenant_id { continue; }
                    let current = match subscription_state.lock() { Ok(current) => current.clone(), Err(_) => return };
                    let Some((target_set, device_set)) = current else { continue; };
                    let device_match = event.device_id.as_ref().is_some_and(|id| device_set.contains(id));
                    let target_match = event.target_id.as_ref().is_some_and(|id| target_set.contains(id));
                    if !device_match && !target_match { continue; }
                    if !principal_is_current(&auth.pool, Some(&pool), &ticket).await { return; }
                    let device_allowed = device_match
                        && event_access_current(
                            &pool,
                            &ticket,
                            "device",
                            event.device_id.as_deref().unwrap_or_default(),
                            None,
                            None,
                        )
                        .await;
                    let target_allowed = target_match
                        && target_projection_current(&pool, &event).await
                        && event_access_current(
                            &pool,
                            &ticket,
                            "target",
                            event.target_id.as_deref().unwrap_or_default(),
                            event.session_id.as_deref(),
                            event.device_id.as_deref(),
                        )
                        .await;
                    if !device_allowed && !target_allowed { continue; }
                    let json = serde_json::to_string(&event.message).unwrap_or_default();
                    match tokio::time::timeout(Duration::from_secs(10), socket.send(Message::Text(json.into()))).await { Ok(Ok(())) => {}, _ => return }
                    continue;
                }
                None => { send_ws_error(&mut socket, "", "INVALID_SUBSCRIPTION", "Slow consumer; reconnect and fetch an authoritative REST snapshot.").await; return; }
            },
            message = socket.recv() => match message { Some(Ok(message)) => message, _ => return }
        };
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(command) = serde_json::from_str::<Subscribe>(&text) else {
            send_ws_error(
                &mut socket,
                "",
                "INVALID_SUBSCRIPTION",
                "Invalid subscription message.",
            )
            .await;
            continue;
        };
        if command.kind != "subscribe"
            || command.request_id.is_empty()
            || command.request_id.len() > 128
        {
            send_ws_error(
                &mut socket,
                &command.request_id,
                "INVALID_SUBSCRIPTION",
                "Invalid subscription request.",
            )
            .await;
            continue;
        }
        let (Ok(targets), Ok(devices)) = (
            canonical_ids(&command.target_ids),
            canonical_ids(&command.device_ids),
        ) else {
            send_ws_error(
                &mut socket,
                &command.request_id,
                "INVALID_SUBSCRIPTION",
                "Subscription IDs must be valid UUIDs.",
            )
            .await;
            continue;
        };
        if targets.len() + devices.len() > 100 {
            send_ws_error(
                &mut socket,
                &command.request_id,
                "INVALID_SUBSCRIPTION",
                "At most 100 subscription IDs are allowed.",
            )
            .await;
            continue;
        }
        let unique: HashSet<&String> = targets.iter().chain(devices.iter()).collect();
        if unique.len() != targets.len() + devices.len() {
            send_ws_error(
                &mut socket,
                &command.request_id,
                "INVALID_SUBSCRIPTION",
                "Duplicate subscription IDs are not allowed.",
            )
            .await;
            continue;
        }
        let Ok(mut tx) = tracking_storage::begin_tenant_transaction(&pool, &ticket.tenant_id).await
        else {
            continue;
        };
        let mut permitted = true;
        for (kind, ids) in [("target", &targets), ("device", &devices)] {
            for id in ids {
                let result = principal_has_permission(&mut tx, &ticket, kind, id).await;
                if !matches!(result, Ok(true)) {
                    permitted = false;
                    break;
                }
            }
        }
        let _ = tx.commit().await;
        if !permitted {
            send_ws_error(
                &mut socket,
                &command.request_id,
                "FORBIDDEN",
                "One or more resources are not accessible.",
            )
            .await;
            continue;
        }
        let subscription_sets = (
            targets.iter().cloned().collect(),
            devices.iter().cloned().collect(),
        );
        if let Ok(mut current) = subscription_state.lock() {
            *current = Some(subscription_sets);
        } else {
            return;
        }
        let current = (targets, devices);
        let ack = Subscribed {
            kind: "subscribed",
            request_id: &command.request_id,
            target_ids: &current.0,
            device_ids: &current.1,
        };
        if !matches!(
            tokio::time::timeout(
                Duration::from_secs(10),
                socket.send(Message::Text(
                    serde_json::to_string(&ack).unwrap_or_default().into()
                ))
            )
            .await,
            Ok(Ok(()))
        ) {
            return;
        }
    }
}

struct ConnectionGuard {
    counts: Arc<Mutex<HashMap<String, usize>>>,
    clients: Arc<Mutex<HashMap<String, RealtimeConnection>>>,
    owner: String,
    connection_id: String,
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Ok(mut clients) = self.clients.lock() {
            clients.remove(&self.connection_id);
        }
        if let Ok(mut counts) = self.counts.lock() {
            if let Some(count) = counts.get_mut(&self.owner) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    counts.remove(&self.owner);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // Event identity/context/data are separate contract fields.
pub(crate) fn publish_event(
    state: &AppState,
    tenant_id: &str,
    device_id: Option<&str>,
    target_id: Option<&str>,
    session_id: Option<&str>,
    event_type: &str,
    observed_at: &str,
    data: serde_json::Value,
) {
    let mut message = serde_json::json!({
        "type": event_type,
        "event_id": uuid::Uuid::new_v4().to_string(),
        "emitted_at": observed_at,
        "data": data,
    });
    if event_type.ends_with("status_changed") || event_type == "device.connectivity_changed" {
        message["observed_at"] = serde_json::Value::String(observed_at.to_owned());
    }
    let event = RealtimeEvent {
        tenant_id: tenant_id.to_owned(),
        device_id: device_id.map(str::to_owned),
        target_id: target_id.map(str::to_owned),
        session_id: session_id.map(str::to_owned),
        message,
    };
    let Ok(mut clients) = state.realtime_clients.lock() else {
        return;
    };
    let mut overflow = Vec::new();
    for (id, client) in clients.iter() {
        if client.tenant_id != event.tenant_id {
            continue;
        }
        let Ok(subscriptions) = client.subscriptions.lock() else {
            continue;
        };
        let Some((targets, devices)) = subscriptions.as_ref() else {
            continue;
        };
        let subscribed = event
            .device_id
            .as_ref()
            .is_some_and(|resource| devices.contains(resource))
            || event
                .target_id
                .as_ref()
                .is_some_and(|resource| targets.contains(resource));
        if subscribed && client.sender.try_send(event.clone()).is_err() {
            overflow.push(id.clone());
        }
    }
    for id in overflow {
        clients.remove(&id);
        state.metrics.record_ws_queue_overflow();
    }
}

pub async fn run_realtime_worker(state: AppState) {
    let (Some(auth), Some(pool)) = (state.auth.clone(), state.tracking_pool.clone()) else {
        return;
    };
    let mut seen = HashMap::<(String, String), (String, String)>::new();
    let mut timer = tokio::time::interval(Duration::from_secs(5));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        timer.tick().await;
        let tenants = match sqlx::query_scalar::<_, String>(
            "SELECT tenant_id::text FROM gps_tracking_active_tenants()",
        )
        .fetch_all(&auth.pool)
        .await
        {
            Ok(rows) => rows,
            Err(_) => continue,
        };
        let mut current_keys = HashSet::new();
        for tenant in tenants {
            let Ok(mut tx) = tracking_storage::begin_tenant_transaction(&pool, &tenant).await
            else {
                continue;
            };
            let rows = sqlx::query_as::<_,(String,String,String,String,String,String)>("SELECT d.id::text,CASE WHEN d.last_contact_at IS NULL THEN 'unknown' WHEN d.last_contact_at>clock_timestamp()-($2::bigint*interval '1 second') THEN 'online' ELSE 'offline' END,COALESCE(to_char(d.last_contact_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),''),CASE WHEN p.id IS NULL THEN 'no_position' WHEN p.recorded_at>clock_timestamp()-($3::bigint*interval '1 second') THEN 'fresh' ELSE 'stale' END,COALESCE(p.recorded_at::text,''),to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') FROM devices d LEFT JOIN LATERAL (SELECT id,recorded_at FROM positions WHERE tenant_id=d.tenant_id AND device_id=d.id AND expires_at>clock_timestamp() ORDER BY recorded_at DESC,id DESC LIMIT 1) p ON TRUE WHERE d.tenant_id=$1::uuid")
                .bind(&tenant).bind(state.device_offline_timeout_seconds).bind(state.position_freshness_seconds).fetch_all(&mut *tx).await;
            let _ = tx.commit().await;
            let Ok(rows) = rows else {
                continue;
            };
            for (device, status, last_contact, position_status, _recorded_at, observed_at) in rows {
                let key = (tenant.clone(), device.clone());
                current_keys.insert(key.clone());
                let old = seen.insert(key, (status.clone(), position_status.clone()));
                if old.as_ref().is_some_and(|value| value.0 != status) {
                    publish_event(
                        &state,
                        &tenant,
                        Some(&device),
                        None,
                        None,
                        "device.connectivity_changed",
                        &observed_at,
                        serde_json::json!({"device_id":device,"connectivity_status":status,"last_contact_at":if last_contact.is_empty(){serde_json::Value::Null}else{serde_json::Value::String(last_contact)}}),
                    );
                }
                if old.as_ref().is_some_and(|value| value.1 != position_status) {
                    publish_event(
                        &state,
                        &tenant,
                        Some(&device),
                        None,
                        None,
                        "position.status_changed",
                        &observed_at,
                        serde_json::json!({"device_id":device,"target_id":null,"session_id":null,"position_status":position_status}),
                    );
                    if let Ok(mut context_tx) =
                        tracking_storage::begin_tenant_transaction(&pool, &tenant).await
                    {
                        let sessions=sqlx::query_as::<_,(String,String)>("SELECT id::text,target_id::text FROM tracking_sessions WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND started_at<=clock_timestamp() AND (ended_at IS NULL OR clock_timestamp()<ended_at)")
                            .bind(&tenant).bind(&device).fetch_all(&mut *context_tx).await.unwrap_or_default();
                        let _ = context_tx.commit().await;
                        for (session, target) in sessions {
                            publish_event(
                                &state,
                                &tenant,
                                Some(&device),
                                Some(&target),
                                Some(&session),
                                "position.status_changed",
                                &observed_at,
                                serde_json::json!({"device_id":device,"target_id":target,"session_id":session,"position_status":position_status}),
                            );
                        }
                    }
                }
            }
        }
        seen.retain(|key, _| current_keys.contains(key));
    }
}

async fn send_ws_error(socket: &mut WebSocket, request_id: &str, code: &str, message: &str) {
    #[derive(Serialize)]
    struct ErrorDetails<'a> {
        code: &'a str,
        message: &'a str,
    }
    #[derive(Serialize)]
    struct ErrorMessage<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        request_id: &'a str,
        error: ErrorDetails<'a>,
    }
    if let Ok(json) = serde_json::to_string(&ErrorMessage {
        kind: "error",
        request_id,
        error: ErrorDetails { code, message },
    }) {
        let _ = socket.send(Message::Text(json.into())).await;
    }
}

fn canonical_ids(values: &[String]) -> Result<Vec<String>, StatusCode> {
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        ids.push(
            uuid::Uuid::parse_str(value)
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .to_string(),
        );
    }
    Ok(ids)
}

fn principal_owner(principal: &RealtimePrincipal) -> &str {
    match principal {
        RealtimePrincipal::Session { user_id, .. } => user_id,
        RealtimePrincipal::Application { owner_user_id, .. } => owner_user_id,
    }
}

async fn principal_is_current(
    auth_pool: &PgPool,
    tracking_pool: Option<&PgPool>,
    ticket: &RealtimeTicket,
) -> bool {
    match &ticket.principal {
        RealtimePrincipal::Session {
            session_id,
            user_id,
        } => {
            let Ok(mut tx) =
                tracking_storage::begin_tenant_transaction(auth_pool, &ticket.tenant_id).await
            else {
                return false;
            };
            let current = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_sessions s JOIN users u ON u.id=s.user_id JOIN memberships m ON m.user_id=u.id JOIN organizations o ON o.id=m.tenant_id WHERE s.id=$1::uuid AND s.user_id=$2::uuid AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() AND s.last_interactive_at>clock_timestamp()-interval '30 minutes' AND s.auth_version=u.auth_version AND u.status='active' AND m.tenant_id=$3::uuid AND m.status='active' AND o.status='active')")
                .bind(session_id).bind(user_id).bind(&ticket.tenant_id).fetch_one(&mut *tx).await.unwrap_or(false);
            tx.commit().await.is_ok() && current
        }
        RealtimePrincipal::Application {
            token_id,
            owner_user_id,
            token_hash,
        } => {
            let Some(pool) = tracking_pool else {
                return false;
            };
            matches!(tracking_storage::lookup_application_token(pool, token_hash).await, Ok(Some(identity)) if identity.token_id == *token_id && identity.owner_user_id == *owner_user_id && identity.tenant_id == ticket.tenant_id && identity.scopes.iter().any(|scope| scope == "tracking:subscribe"))
        }
    }
}

async fn event_access_current(
    pool: &PgPool,
    ticket: &RealtimeTicket,
    kind: &str,
    resource_id: &str,
    session_id: Option<&str>,
    device_id: Option<&str>,
) -> bool {
    let Ok(mut tx) = tracking_storage::begin_tenant_transaction(pool, &ticket.tenant_id).await
    else {
        return false;
    };
    let permitted = principal_has_permission(&mut tx, ticket, kind, resource_id).await;
    let same_session = if kind == "target" {
        if let Some(session_id) = session_id {
            sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM tracking_sessions WHERE tenant_id=$1::uuid AND id=$2::uuid AND target_id=$3::uuid AND ($4::uuid IS NULL OR device_id=$4::uuid))")
                .bind(&ticket.tenant_id).bind(session_id).bind(resource_id).bind(device_id).fetch_one(&mut *tx).await.unwrap_or(false)
        } else {
            true
        }
    } else {
        true
    };
    let _ = tx.commit().await;
    same_session && matches!(permitted, Ok(true))
}

async fn target_projection_current(pool: &PgPool, event: &RealtimeEvent) -> bool {
    let event_type = event.message["type"].as_str().unwrap_or_default();
    if !matches!(event_type, "position.updated" | "position.status_changed") {
        return true;
    }
    let (Some(target_id), Some(session_id), Some(device_id)) = (
        event.target_id.as_deref(),
        event.session_id.as_deref(),
        event.device_id.as_deref(),
    ) else {
        return false;
    };
    let recorded_at = event.message["data"]["position"]["recorded_at"].as_str();
    let Ok(mut tx) = tracking_storage::begin_tenant_transaction(pool, &event.tenant_id).await
    else {
        return false;
    };
    let current = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM tracking_sessions WHERE tenant_id=$1::uuid AND id=$2::uuid AND target_id=$3::uuid AND device_id=$4::uuid AND ended_at IS NULL AND started_at<=clock_timestamp() AND ($5::timestamptz IS NULL OR started_at<=$5::timestamptz))",
    )
    .bind(&event.tenant_id)
    .bind(session_id)
    .bind(target_id)
    .bind(device_id)
    .bind(recorded_at)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(false);
    let _ = tx.commit().await;
    current
}

async fn principal_has_permission(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ticket: &RealtimeTicket,
    kind: &str,
    resource_id: &str,
) -> Result<bool, sqlx::Error> {
    match &ticket.principal {
        RealtimePrincipal::Session { user_id, .. } => sqlx::query_scalar("SELECT (CASE WHEN $3='target' THEN EXISTS(SELECT 1 FROM targets WHERE tenant_id=$1::uuid AND id=$4::uuid) ELSE EXISTS(SELECT 1 FROM devices WHERE tenant_id=$1::uuid AND id=$4::uuid) END) AND gps_tracking_member_has_permission($1::uuid,$2::uuid,$3,$4::uuid,'read')").bind(&ticket.tenant_id).bind(user_id).bind(kind).bind(resource_id).fetch_one(&mut **tx).await,
        RealtimePrincipal::Application { token_id, owner_user_id, .. } => sqlx::query_scalar("SELECT (CASE WHEN $3='target' THEN EXISTS(SELECT 1 FROM targets WHERE tenant_id=$2::uuid AND id=$4::uuid) ELSE EXISTS(SELECT 1 FROM devices WHERE tenant_id=$2::uuid AND id=$4::uuid) END) AND gps_tracking_application_token_owner_has_permission($1::uuid,$2::uuid,$3,$4::uuid,'read') AND gps_tracking_member_has_permission($2::uuid,$5::uuid,$3,$4::uuid,'read')").bind(token_id).bind(&ticket.tenant_id).bind(kind).bind(resource_id).bind(owner_user_id).fetch_one(&mut **tx).await,
    }
}

async fn subscriptions_current(
    pool: &PgPool,
    ticket: &RealtimeTicket,
    subscriptions: &SharedRealtimeSubscriptions,
) -> bool {
    let selected = match subscriptions.lock() {
        Ok(current) => current.clone(),
        Err(_) => return false,
    };
    let Some((targets, devices)) = selected else {
        return true;
    };
    let Ok(mut tx) = tracking_storage::begin_tenant_transaction(pool, &ticket.tenant_id).await
    else {
        return false;
    };
    for (kind, ids) in [("target", targets), ("device", devices)] {
        for id in ids {
            if !matches!(
                principal_has_permission(&mut tx, ticket, kind, &id).await,
                Ok(true)
            ) {
                let _ = tx.rollback().await;
                return false;
            }
        }
    }
    tx.commit().await.is_ok()
}
