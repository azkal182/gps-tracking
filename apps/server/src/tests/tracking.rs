use super::*;
use crate::tracking::PositionHistoryQuery;
use axum::extract::Path;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn live02_websocket_ticket_subscription_and_revoke() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let auth_role = format!("live02_auth_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {auth_role} LOGIN PASSWORD 'live02-auth-disposable'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT gps_tracking_auth TO {auth_role}"))
        .execute(&pool)
        .await
        .unwrap();
    let auth_pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE gps_tracking_auth")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&auth_role)
                .password("live02-auth-disposable"),
        )
        .await
        .unwrap();
    let role = format!("live02_app_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'live02-disposable'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT gps_tracking_app TO {role}"))
        .execute(&pool)
        .await
        .unwrap();
    let tracking_pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE gps_tracking_app")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&role)
                .password("live02-disposable"),
        )
        .await
        .unwrap();
    let mut state = AppState::with_auth_test_key(
        auth_pool.clone(),
        "https://portal.example".into(),
        [114; 32],
    );
    state.set_tracking_pool(tracking_pool.clone());
    let tenant = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'LIVE-02 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    let user = insert_org01_user(&pool, &format!("live02-{suffix}@example.invalid"), false).await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')").bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&user).execute(&pool).await.unwrap();
    let target = uuid::Uuid::new_v4().to_string();
    let device = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'LIVE-02 target')",
    )
    .bind(&target)
    .bind(&tenant)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'LIVE-02 device','http-json',$3)").bind(&device).bind(&tenant).bind(format!("live02-{suffix}")).execute(&pool).await.unwrap();
    let (cookie, csrf) = insert_org01_session(&pool, &user, false).await;
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    headers.insert("x-csrf-token", csrf.parse().unwrap());
    headers.insert("x-organization-id", tenant.parse().unwrap());
    let ticket_response = create_realtime_ticket(
        State(state.clone()),
        headers.clone(),
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let ticket_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(ticket_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let ticket = ticket_json["ticket"].as_str().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut event_state = state.clone();
    event_state.set_tracking_status_thresholds(60, 180);
    let server = tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let (expired_auth_cookie, expired_auth_csrf) = insert_org01_session(&pool, &user, false).await;
    let mut expired_auth_headers = headers.clone();
    expired_auth_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={expired_auth_cookie}")
            .parse()
            .unwrap(),
    );
    expired_auth_headers.insert("x-csrf-token", expired_auth_csrf.parse().unwrap());
    let expired_auth_ticket_response = create_realtime_ticket(
        State(event_state.clone()),
        expired_auth_headers,
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let expired_auth_ticket_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(expired_auth_ticket_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    sqlx::query("UPDATE auth_sessions SET created_at=clock_timestamp()-interval '1 day', expires_at=clock_timestamp()-interval '1 second' WHERE secret_hash=$1")
        .bind(digest(&expired_auth_cookie))
        .execute(&pool)
        .await
        .unwrap();
    let expired_auth_ticket = expired_auth_ticket_json["ticket"].as_str().unwrap();
    let mut expired_auth_request =
        format!("ws://{address}/v1/ws/tracking?ticket={expired_auth_ticket}")
            .into_client_request()
            .unwrap();
    expired_auth_request
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    assert!(
        tokio_tungstenite::connect_async(expired_auth_request)
            .await
            .is_err(),
        "an expired login session must not authorize a WebSocket upgrade"
    );
    let wrong_origin_response = create_realtime_ticket(
        State(event_state.clone()),
        headers.clone(),
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let wrong_origin_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(wrong_origin_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let mut wrong_origin = format!(
        "ws://{address}/v1/ws/tracking?ticket={}",
        wrong_origin_json["ticket"].as_str().unwrap()
    )
    .into_client_request()
    .unwrap();
    wrong_origin
        .headers_mut()
        .insert(header::ORIGIN, "https://evil.example".parse().unwrap());
    assert!(
        tokio_tungstenite::connect_async(wrong_origin)
            .await
            .is_err(),
        "wrong browser Origin must be rejected"
    );
    let expired_response = create_realtime_ticket(
        State(event_state.clone()),
        headers.clone(),
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let expired_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(expired_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let expired_ticket = expired_json["ticket"].as_str().unwrap();
    let expired_hash = URL_SAFE_NO_PAD.encode(digest(expired_ticket));
    event_state
        .realtime_tickets
        .lock()
        .unwrap()
        .get_mut(&expired_hash)
        .unwrap()
        .expires_at = Instant::now() - Duration::from_secs(1);
    let mut expired_request = format!("ws://{address}/v1/ws/tracking?ticket={expired_ticket}")
        .into_client_request()
        .unwrap();
    expired_request
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    assert!(
        tokio_tungstenite::connect_async(expired_request)
            .await
            .is_err(),
        "expired ticket must fail"
    );
    let restart_response = create_realtime_ticket(
        State(event_state.clone()),
        headers.clone(),
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let restart_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(restart_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let restart_state =
        AppState::with_auth_test_key(pool.clone(), "https://portal.example".into(), [114; 32]);
    let mut restart_state = restart_state;
    restart_state.set_tracking_pool(tracking_pool.clone());
    let restart_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let restart_address = restart_listener.local_addr().unwrap();
    let restarted_server = tokio::spawn(async move {
        axum::serve(restart_listener, app(restart_state))
            .await
            .unwrap()
    });
    let mut restart_request = format!(
        "ws://{restart_address}/v1/ws/tracking?ticket={}",
        restart_json["ticket"].as_str().unwrap()
    )
    .into_client_request()
    .unwrap();
    restart_request
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    assert!(
        tokio_tungstenite::connect_async(restart_request)
            .await
            .is_err(),
        "tickets must not survive process restart"
    );
    restarted_server.abort();
    let url = format!("ws://{address}/v1/ws/tracking?ticket={ticket}");
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let replay_url = format!("ws://{address}/v1/ws/tracking?ticket={ticket}");
    let mut replay = replay_url.into_client_request().unwrap();
    replay
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    assert!(
        tokio_tungstenite::connect_async(replay).await.is_err(),
        "single-use ticket replay must fail"
    );
    socket.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"type":"subscribe","request_id":"ok","target_ids":[target],"device_ids":[device]}).to_string().into())).await.unwrap();
    let ack = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(ack.to_text().unwrap().contains("subscribed"));
    let foreign_tenant = uuid::Uuid::new_v4().to_string();
    let foreign_target = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'LIVE-02 foreign tenant')")
        .bind(&foreign_tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'LIVE-02 foreign target')",
    )
    .bind(&foreign_target)
    .bind(&foreign_tenant)
    .execute(&pool)
    .await
    .unwrap();
    socket.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"type":"subscribe","request_id":"denied","target_ids":[foreign_target],"device_ids":[]}).to_string().into())).await.unwrap();
    let denial = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let denial_json: serde_json::Value = serde_json::from_str(denial.to_text().unwrap()).unwrap();
    assert_eq!(
        denial_json["error"]["code"], "FORBIDDEN",
        "denial={denial_json}"
    );
    publish_event(
        &event_state,
        &tenant,
        Some(&device),
        Some(&target),
        None,
        "position.updated",
        "2026-10-19T00:00:00Z",
        serde_json::json!({"device_id":device,"target_id":target}),
    );
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        event.to_text().unwrap().contains("position.updated"),
        "authorized previous subscription must remain after denied replacement"
    );
    let worker = tokio::spawn(run_realtime_worker(event_state.clone()));
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    sqlx::query("UPDATE devices SET last_contact_at=clock_timestamp() WHERE tenant_id=$1::uuid AND id=$2::uuid").bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let online = tokio::time::timeout(std::time::Duration::from_secs(6), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        online
            .to_text()
            .unwrap()
            .contains("device.connectivity_changed")
    );
    assert!(
        online.to_text().unwrap().contains("online"),
        "connectivity event={}",
        online.to_text().unwrap()
    );
    sqlx::query("UPDATE devices SET last_contact_at=clock_timestamp()-interval '5 minutes' WHERE tenant_id=$1::uuid AND id=$2::uuid").bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let offline = tokio::time::timeout(std::time::Duration::from_secs(6), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        offline
            .to_text()
            .unwrap()
            .contains("device.connectivity_changed")
    );
    assert!(offline.to_text().unwrap().contains("offline"));
    let application_token_id = uuid::Uuid::new_v4().to_string();
    let application_secret = format!("gps_app_live02_{suffix}");
    let mut token_tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) VALUES($1::uuid,$2::uuid,$3::uuid,'LIVE-02 synthetic token',$4,$5,clock_timestamp()+interval '1 hour')")
        .bind(&application_token_id).bind(&tenant).bind(&user).bind(digest(&application_secret)).bind(vec!["tracking:subscribe".to_owned(),"targets:read".to_owned()]).execute(&mut *token_tx).await.unwrap();
    sqlx::query("INSERT INTO application_token_targets(tenant_id,application_token_id,target_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
        .bind(&tenant).bind(&application_token_id).bind(&target).execute(&mut *token_tx).await.unwrap();
    token_tx.commit().await.unwrap();
    let mut application_headers = HeaderMap::new();
    application_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {application_secret}").parse().unwrap(),
    );
    let app_ticket_response = create_realtime_ticket(
        State(event_state.clone()),
        application_headers,
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let app_ticket_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(app_ticket_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let app_url = format!(
        "ws://{address}/v1/ws/tracking?ticket={}",
        app_ticket_json["ticket"].as_str().unwrap()
    );
    let (mut app_socket, _) = tokio_tungstenite::connect_async(app_url).await.unwrap();
    app_socket.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"type":"subscribe","request_id":"app-sub","target_ids":[target],"device_ids":[]}).to_string().into())).await.unwrap();
    let app_ack = tokio::time::timeout(std::time::Duration::from_secs(2), app_socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(app_ack.to_text().unwrap().contains("subscribed"));
    let mut direct_request = format!("ws://{address}/v1/ws/tracking")
        .into_client_request()
        .unwrap();
    direct_request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {application_secret}").parse().unwrap(),
    );
    let (mut direct_socket, _) = tokio_tungstenite::connect_async(direct_request)
        .await
        .unwrap();
    direct_socket.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"type":"subscribe","request_id":"direct-app-sub","target_ids":[target],"device_ids":[]}).to_string().into())).await.unwrap();
    let direct_ack = tokio::time::timeout(std::time::Duration::from_secs(2), direct_socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(direct_ack.to_text().unwrap().contains("subscribed"));
    sqlx::query("UPDATE application_tokens SET revoked_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&application_token_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(6), app_socket.next())
            .await
            .is_ok(),
        "revoked application token must close its open socket"
    );
    let _ = app_socket.close(None).await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(6), direct_socket.next())
            .await
            .is_ok(),
        "revoked direct bearer must close its open socket"
    );
    let _ = direct_socket.close(None).await;
    let reader = insert_org01_user(
        &pool,
        &format!("live02-reader-{suffix}@example.invalid"),
        false,
    )
    .await;
    let membership_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'viewer','active')").bind(&membership_id).bind(&tenant).bind(&reader).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO object_grants(id,tenant_id,membership_id,resource_type,target_id,permission) VALUES($1::uuid,$2::uuid,$3::uuid,'target',$4::uuid,'read')").bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&membership_id).bind(&target).execute(&pool).await.unwrap();
    let (reader_cookie, reader_csrf) = insert_org01_session(&pool, &reader, false).await;
    let mut reader_headers = HeaderMap::new();
    reader_headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    reader_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={reader_cookie}")
            .parse()
            .unwrap(),
    );
    reader_headers.insert("x-csrf-token", reader_csrf.parse().unwrap());
    reader_headers.insert("x-organization-id", tenant.parse().unwrap());
    let reader_ticket_response = create_realtime_ticket(
        State(event_state.clone()),
        reader_headers,
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let reader_ticket_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(reader_ticket_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let reader_url = format!(
        "ws://{address}/v1/ws/tracking?ticket={}",
        reader_ticket_json["ticket"].as_str().unwrap()
    );
    let mut reader_request = reader_url.into_client_request().unwrap();
    reader_request
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    let (mut reader_socket, _) = tokio_tungstenite::connect_async(reader_request)
        .await
        .unwrap();
    reader_socket.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::json!({"type":"subscribe","request_id":"reader-sub","target_ids":[target],"device_ids":[]}).to_string().into())).await.unwrap();
    let reader_ack = tokio::time::timeout(std::time::Duration::from_secs(2), reader_socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(reader_ack.to_text().unwrap().contains("subscribed"));
    sqlx::query("DELETE FROM object_grants WHERE tenant_id=$1::uuid AND membership_id=$2::uuid")
        .bind(&tenant)
        .bind(&membership_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(6), reader_socket.next())
            .await
            .is_ok(),
        "grant revocation must close an open subscription"
    );
    let _ = reader_socket.close(None).await;
    sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE secret_hash=$1")
        .bind(digest(&cookie))
        .execute(&pool)
        .await
        .unwrap();
    let closed = tokio::time::timeout(std::time::Duration::from_secs(6), socket.next()).await;
    assert!(
        closed.is_ok(),
        "open socket must close after auth revocation"
    );
    let _ = socket.close(None).await;
    server.abort();
    worker.abort();
    auth_pool.close().await;
    tracking_pool.close().await;
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn live01_heartbeat_contact_status_freshness_and_lifecycle() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("live01_app_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'live01-disposable'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT gps_tracking_app TO {role}"))
        .execute(&pool)
        .await
        .unwrap();
    let tracking_pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE gps_tracking_app")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&role)
                .password("live01-disposable"),
        )
        .await
        .unwrap();
    let mut state =
        AppState::with_auth_test_key(pool.clone(), "https://portal.example".into(), [113; 32]);
    state.set_tracking_pool(tracking_pool.clone());

    let tenant = uuid::Uuid::new_v4().to_string();
    let target = uuid::Uuid::new_v4().to_string();
    let device = uuid::Uuid::new_v4().to_string();
    let secret = format!("gps_live_live01_{suffix}");
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'LIVE-01 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    let user = insert_org01_user(&pool, &format!("live01-{suffix}@example.invalid"), false).await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&user).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'LIVE-01 target')",
    )
    .bind(&target)
    .bind(&tenant)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier,ingestion_policy) VALUES($1::uuid,$2::uuid,'LIVE-01 device','http-json',$3,'session_only')")
        .bind(&device).bind(&tenant).bind(format!("live01-{suffix}")).execute(&pool).await.unwrap();
    let credential = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO device_credentials(id,tenant_id,device_id,token_hash) VALUES($1::uuid,$2::uuid,$3::uuid,$4)")
        .bind(&credential).bind(&tenant).bind(&device).bind(digest(&secret)).execute(&pool).await.unwrap();
    let (cookie, csrf) = insert_org01_session(&pool, &user, false).await;
    let mut portal_headers = HeaderMap::new();
    portal_headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    portal_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    portal_headers.insert("x-csrf-token", csrf.parse().unwrap());
    portal_headers.insert("x-organization-id", tenant.parse().unwrap());
    let mut device_headers = HeaderMap::new();
    device_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {secret}").parse().unwrap(),
    );
    device_headers.insert("x-request-id", "live01-synthetic-request".parse().unwrap());

    let initial = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let initial: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(initial.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(initial["connectivity_status"], "unknown");
    assert_eq!(initial["position_status"], "no_position");
    let before_positions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM positions WHERE tenant_id=$1::uuid AND device_id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&device)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before_positions, 0);

    let first = ingest_heartbeat(
        State(state.clone()),
        device_headers.clone(),
        Json(HeartbeatRequest {}),
    )
    .await
    .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first.headers().get("x-request-id").unwrap(),
        "live01-synthetic-request"
    );
    assert_eq!(
        first.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let first: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(first.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["device_id"], device);
    assert_eq!(first["connectivity_status"], "online");
    let after_heartbeat = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let after_heartbeat: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(after_heartbeat.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(after_heartbeat["connectivity_status"], "online");
    assert_eq!(after_heartbeat["position_status"], "no_position");
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM positions WHERE tenant_id=$1::uuid AND device_id=$2::uuid),(SELECT count(*) FROM tracking_sessions WHERE tenant_id=$1::uuid AND device_id=$2::uuid)")
        .bind(&tenant).bind(&device).fetch_one(&pool).await.unwrap();
    assert_eq!(
        counts,
        (0, 0),
        "heartbeat does not add positions or create a tracking session"
    );

    // A stale stored sample remains stale when heartbeat refreshes contact.
    sqlx::query("INSERT INTO positions(tenant_id,device_id,message_id,payload_fingerprint,location,recorded_at) VALUES($1::uuid,$2::uuid,'live01-stale',$3,ST_SetSRID(ST_MakePoint(110,-6),4326)::geography,clock_timestamp()-interval '61 seconds')")
        .bind(&tenant).bind(&device).bind(vec![7u8;32]).execute(&pool).await.unwrap();
    let stale_before = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let stale_before: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(stale_before.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stale_before["position_status"], "stale");
    let _ = ingest_heartbeat(
        State(state.clone()),
        device_headers.clone(),
        Json(HeartbeatRequest {}),
    )
    .await
    .unwrap();
    let stale_after = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let stale_after: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(stale_after.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stale_after["connectivity_status"], "online");
    assert_eq!(
        stale_after["position_status"], "stale",
        "contact does not refresh recorded_at or position freshness"
    );
    sqlx::query("UPDATE positions SET recorded_at=clock_timestamp()-interval '59 seconds' WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND message_id='live01-stale'")
        .bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let fresh = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let fresh: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(fresh.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        fresh["position_status"], "fresh",
        "position age below 60 seconds is fresh"
    );
    sqlx::query("UPDATE positions SET recorded_at=clock_timestamp()-interval '60 seconds' WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND message_id='live01-stale'")
        .bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let stale_boundary = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let stale_boundary: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(stale_boundary.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        stale_boundary["position_status"], "stale",
        "position age at 60 seconds is stale"
    );

    let rejected_recorded_at: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()-interval '5 seconds') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&pool).await.unwrap();
    let rejected_batch = serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
        "device_id": device,
        "points": [{"message_id":"live01-outside-session","latitude":-6.8,"longitude":110.0,"recorded_at":rejected_recorded_at}]
    })).unwrap();
    let mut rejected_headers = HeaderMap::new();
    rejected_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {secret}").parse().unwrap(),
    );
    let rejected = ingest_positions(State(state.clone()), rejected_headers, Json(rejected_batch))
        .await
        .unwrap()
        .into_response();
    let rejected: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(rejected.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(rejected["summary"]["rejected"], 1);
    let contact_after_rejection: bool = sqlx::query_scalar("SELECT last_contact_at>clock_timestamp()-interval '5 seconds' FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid")
        .bind(&tenant).bind(&device).fetch_one(&pool).await.unwrap();
    assert!(
        contact_after_rejection,
        "an authenticated, structurally valid all-rejected batch still proves live contact"
    );

    // Concurrent heartbeat and an out-of-order contact timestamp preserve monotonicity.
    state.heartbeat_rate_limit.lock().unwrap().clear();
    let concurrent_a = ingest_heartbeat(
        State(state.clone()),
        device_headers.clone(),
        Json(HeartbeatRequest {}),
    );
    let concurrent_b = ingest_heartbeat(
        State(state.clone()),
        device_headers.clone(),
        Json(HeartbeatRequest {}),
    );
    let (concurrent_a, concurrent_b) = tokio::join!(concurrent_a, concurrent_b);
    assert_eq!(concurrent_a.unwrap().status(), StatusCode::OK);
    assert_eq!(concurrent_b.unwrap().status(), StatusCode::OK);
    sqlx::query("UPDATE devices SET last_contact_at=clock_timestamp()+interval '1 hour' WHERE tenant_id=$1::uuid AND id=$2::uuid")
        .bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let monotonic = ingest_heartbeat(
        State(state.clone()),
        device_headers.clone(),
        Json(HeartbeatRequest {}),
    )
    .await
    .unwrap();
    let monotonic: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(monotonic.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let future_contact_preserved: bool = sqlx::query_scalar("SELECT last_contact_at > clock_timestamp()+interval '59 minutes' FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid")
        .bind(&tenant).bind(&device).fetch_one(&pool).await.unwrap();
    assert!(future_contact_preserved);
    assert_eq!(monotonic["connectivity_status"], "online");

    for (age, expected) in [(179, "online"), (180, "offline")] {
        sqlx::query("UPDATE devices SET last_contact_at=clock_timestamp()-($3::text || ' seconds')::interval WHERE tenant_id=$1::uuid AND id=$2::uuid")
            .bind(&tenant).bind(&device).bind(age.to_string()).execute(&pool).await.unwrap();
        let snapshot = get_device_location(
            State(state.clone()),
            Path(device.clone()),
            portal_headers.clone(),
        )
        .await
        .unwrap();
        let snapshot: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(snapshot.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            snapshot["connectivity_status"], expected,
            "offline threshold is exact at {age}s"
        );
        assert_eq!(snapshot["position_status"], "stale");
    }
    state.set_tracking_status_thresholds(30, 240);
    sqlx::query("UPDATE devices SET last_contact_at=clock_timestamp()-interval '239 seconds' WHERE tenant_id=$1::uuid AND id=$2::uuid")
        .bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let configured_online = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let configured_online: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(configured_online.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        configured_online["connectivity_status"], "online",
        "runtime status uses configured timeout"
    );
    sqlx::query("UPDATE devices SET last_contact_at=clock_timestamp()-interval '240 seconds' WHERE tenant_id=$1::uuid AND id=$2::uuid")
        .bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let configured_offline = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let configured_offline: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(configured_offline.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(configured_offline["connectivity_status"], "offline");
    sqlx::query("UPDATE positions SET recorded_at=clock_timestamp()-interval '29 seconds' WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND message_id='live01-stale'")
        .bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let configured_fresh = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let configured_fresh: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(configured_fresh.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        configured_fresh["position_status"], "fresh",
        "runtime position freshness uses configured threshold"
    );
    sqlx::query("UPDATE positions SET recorded_at=clock_timestamp()-interval '30 seconds' WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND message_id='live01-stale'")
        .bind(&tenant).bind(&device).execute(&pool).await.unwrap();
    let configured_stale = get_device_location(
        State(state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let configured_stale: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(configured_stale.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(configured_stale["position_status"], "stale");

    let rejected_body = serde_json::from_str::<HeartbeatRequest>(r#"{"latitude":-6.8}"#);
    assert!(
        rejected_body.is_err(),
        "heartbeat schema rejects coordinates and other fields"
    );
    sqlx::query("UPDATE devices SET enabled=false WHERE tenant_id=$1::uuid AND id=$2::uuid")
        .bind(&tenant)
        .bind(&device)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        ingest_heartbeat(
            State(state.clone()),
            device_headers.clone(),
            Json(HeartbeatRequest {})
        )
        .await
        .err(),
        Some(StatusCode::UNAUTHORIZED)
    );
    sqlx::query("UPDATE devices SET enabled=true WHERE tenant_id=$1::uuid AND id=$2::uuid")
        .bind(&tenant)
        .bind(&device)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&credential)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        ingest_heartbeat(
            State(state.clone()),
            device_headers.clone(),
            Json(HeartbeatRequest {})
        )
        .await
        .err(),
        Some(StatusCode::UNAUTHORIZED)
    );

    // A fresh, separate device exercises the configured 3-token burst and 6/min refill rate.
    let rate_device = uuid::Uuid::new_v4().to_string();
    let rate_secret = format!("gps_live_rate_{suffix}");
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier,ingestion_policy) VALUES($1::uuid,$2::uuid,'LIVE-01 rate device','http-json',$3,'always')")
        .bind(&rate_device).bind(&tenant).bind(format!("live01-rate-{suffix}")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO device_credentials(id,tenant_id,device_id,token_hash) VALUES($1::uuid,$2::uuid,$3::uuid,$4)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&rate_device).bind(digest(&rate_secret)).execute(&pool).await.unwrap();
    device_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {rate_secret}").parse().unwrap(),
    );
    for _ in 0..3 {
        assert_eq!(
            ingest_heartbeat(
                State(state.clone()),
                device_headers.clone(),
                Json(HeartbeatRequest {})
            )
            .await
            .unwrap()
            .status(),
            StatusCode::OK
        );
    }
    let limited = ingest_heartbeat(
        State(state.clone()),
        device_headers,
        Json(HeartbeatRequest {}),
    )
    .await
    .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = limited
        .headers()
        .get(header::RETRY_AFTER)
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=10).contains(&retry_after));
    tracking_pool.close().await;
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn track03_latest_history_cursor_expiry_and_session_visibility() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("track03_app_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'track03-disposable'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT gps_tracking_app TO {role}"))
        .execute(&pool)
        .await
        .unwrap();
    let app_pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE gps_tracking_app")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&role)
                .password("track03-disposable"),
        )
        .await
        .unwrap();
    let auth_pool = pool.clone();
    let mut state =
        AppState::with_auth_test_key(auth_pool, "https://portal.example".into(), [112; 32]);
    state.set_tracking_pool(app_pool);

    let tenant = uuid::Uuid::new_v4().to_string();
    let second_tenant = uuid::Uuid::new_v4().to_string();
    let target = uuid::Uuid::new_v4().to_string();
    let device = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'TRACK-03 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'TRACK-03 second fixture')")
        .bind(&second_tenant)
        .execute(&pool)
        .await
        .unwrap();
    let user = insert_org01_user(&pool, &format!("track03-{suffix}@example.invalid"), false).await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&user).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&second_tenant).bind(&user).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'TRACK-03 target')",
    )
    .bind(&target)
    .bind(&tenant)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'TRACK-03 device','http-json',$3)")
        .bind(&device).bind(&tenant).bind(format!("track03-{suffix}")).execute(&pool).await.unwrap();
    let old_session = uuid::Uuid::new_v4().to_string();
    let active_session = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tracking_sessions(id,tenant_id,target_id,device_id,started_at,ended_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,clock_timestamp()-interval '4 days',clock_timestamp()-interval '2 days'),($5::uuid,$2::uuid,$3::uuid,$4::uuid,clock_timestamp()-interval '1 day',NULL)")
        .bind(&old_session).bind(&tenant).bind(&target).bind(&device).bind(&active_session).execute(&pool).await.unwrap();
    // Deliberately insert in non-chronological order, with a timestamp tie, an end-boundary point and an expired point.
    sqlx::query("INSERT INTO positions(tenant_id,device_id,message_id,payload_fingerprint,location,recorded_at) VALUES
        ($1::uuid,$2::uuid,$3::uuid,$4,ST_SetSRID(ST_MakePoint(110,-6),4326)::geography,date_trunc('second',clock_timestamp()-interval '3 days')),
        ($1::uuid,$2::uuid,$5::uuid,$6,ST_SetSRID(ST_MakePoint(111,-6),4326)::geography,date_trunc('second',clock_timestamp()-interval '3 days')),
        ($1::uuid,$2::uuid,$7::uuid,$8,ST_SetSRID(ST_MakePoint(108,-6),4326)::geography,clock_timestamp()-interval '2 days'),
        ($1::uuid,$2::uuid,$9::uuid,$10,ST_SetSRID(ST_MakePoint(107,-6),4326)::geography,clock_timestamp()-interval '400 days')")
        .bind(&tenant).bind(&device)
        .bind(uuid::Uuid::new_v4().to_string()).bind(vec![1u8;32]).bind(uuid::Uuid::new_v4().to_string()).bind(vec![2u8;32])
        .bind(uuid::Uuid::new_v4().to_string()).bind(vec![3u8;32]).bind(uuid::Uuid::new_v4().to_string()).bind(vec![4u8;32])
        .execute(&pool).await.unwrap();
    let (cookie, csrf) = insert_org01_session(&pool, &user, false).await;
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    headers.insert("x-csrf-token", csrf.parse().unwrap());
    headers.insert("x-organization-id", tenant.parse().unwrap());

    let latest = get_target_location(State(state.clone()), Path(target.clone()), headers.clone())
        .await
        .unwrap();
    let latest: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(latest.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(latest["tracking_status"], "active");
    assert_eq!(
        latest["position_status"], "no_position",
        "previous-session positions must not leak into target latest"
    );
    let device_latest =
        get_device_location(State(state.clone()), Path(device.clone()), headers.clone())
            .await
            .unwrap();
    let device_latest: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(device_latest.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        device_latest["position"]["longitude"], 108.0,
        "device latest includes saved positions independent from session membership"
    );

    let page = get_device_positions(
        State(state.clone()),
        Path(device.clone()),
        Query(PositionHistoryQuery {
            from: Some("2026-10-01T00:00:00Z".into()),
            to: Some("2026-10-31T00:00:00Z".into()),
            limit: Some(1),
            cursor: None,
        }),
        headers.clone(),
    )
    .await
    .unwrap();
    let page: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(page.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page["data"].as_array().unwrap().len(), 1);
    assert_eq!(page["data"][0]["longitude"], 108.0);
    let cursor = page["next_cursor"].as_str().unwrap().to_owned();
    let next = get_device_positions(
        State(state.clone()),
        Path(device.clone()),
        Query(PositionHistoryQuery {
            from: None,
            to: None,
            limit: Some(1),
            cursor: Some(cursor.clone()),
        }),
        headers.clone(),
    )
    .await
    .unwrap();
    let next: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(next.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        next["data"][0]["longitude"], 111.0,
        "equal timestamps sort by descending position id"
    );
    assert_eq!(
        get_device_positions(
            State(state.clone()),
            Path(uuid::Uuid::new_v4().to_string()),
            Query(PositionHistoryQuery {
                from: None,
                to: None,
                limit: Some(1),
                cursor: Some(cursor.clone()),
            }),
            headers.clone(),
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST),
        "cursor is bound to tenant, resource and filters"
    );
    let mut other_tenant_headers = headers.clone();
    other_tenant_headers.insert("x-organization-id", second_tenant.parse().unwrap());
    assert_eq!(
        get_device_positions(
            State(state.clone()),
            Path(device.clone()),
            Query(PositionHistoryQuery {
                from: None,
                to: None,
                limit: Some(1),
                cursor: Some(cursor.clone())
            }),
            other_tenant_headers,
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST),
        "cursor cannot cross tenant scope"
    );
    assert_eq!(
        get_device_positions(
            State(state.clone()),
            Path(device.clone()),
            Query(PositionHistoryQuery {
                from: Some("2026-10-01T00:00:00Z".into()),
                to: Some("2026-10-30T00:00:00Z".into()),
                limit: Some(1),
                cursor: Some(cursor),
            }),
            headers.clone()
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST),
        "cursor cannot be reused with different time bounds"
    );
    let session_history = get_session_positions(
        State(state.clone()),
        Path(old_session.clone()),
        Query(PositionHistoryQuery {
            from: Some("2026-10-01T00:00:00Z".into()),
            to: Some("2026-10-31T00:00:00Z".into()),
            limit: Some(100),
            cursor: None,
        }),
        headers,
    )
    .await
    .unwrap();
    let session_history: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(session_history.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        session_history["data"].as_array().unwrap().len(),
        2,
        "end timestamp is exclusive; the expired fixture falls outside the requested window"
    );
    let leap_year_expiry: bool = sqlx::query_scalar(
        "SELECT position_expires_at_utc('2024-02-29T12:00:00Z'::timestamptz)='2025-02-28T12:00:00Z'::timestamptz",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        leap_year_expiry,
        "retention uses a UTC calendar year, including leap-day normalization"
    );
    let position_permission_function: Option<String> = sqlx::query_scalar(
        "SELECT to_regprocedure('public.gps_tracking_application_token_owner_has_position_permission(uuid,uuid,text,uuid)')::text",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(position_permission_function.is_some());
    let position_secret = format!("gps_app_{}", random_secret());
    let position_token = uuid::Uuid::new_v4().to_string();
    let wrong_scope_secret = format!("gps_app_{}", random_secret());
    let wrong_scope_token = uuid::Uuid::new_v4().to_string();
    let mut token_tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) VALUES($1::uuid,$2::uuid,$3::uuid,'TRACK-03 position read',$4,ARRAY['positions:read'],clock_timestamp()+interval '1 day')")
        .bind(&position_token).bind(&tenant).bind(&user).bind(digest(&position_secret)).execute(&mut *token_tx).await.unwrap();
    sqlx::query("INSERT INTO application_token_devices(tenant_id,application_token_id,device_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
        .bind(&tenant).bind(&position_token).bind(&device).execute(&mut *token_tx).await.unwrap();
    sqlx::query("INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) VALUES($1::uuid,$2::uuid,$3::uuid,'TRACK-03 wrong scope',$4,ARRAY['sessions:read'],clock_timestamp()+interval '1 day')")
        .bind(&wrong_scope_token).bind(&tenant).bind(&user).bind(digest(&wrong_scope_secret)).execute(&mut *token_tx).await.unwrap();
    sqlx::query("INSERT INTO application_token_devices(tenant_id,application_token_id,device_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
        .bind(&tenant).bind(&wrong_scope_token).bind(&device).execute(&mut *token_tx).await.unwrap();
    token_tx.commit().await.unwrap();
    let mut position_headers = HeaderMap::new();
    position_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {position_secret}").parse().unwrap(),
    );
    position_headers.insert("x-organization-id", tenant.parse().unwrap());
    assert_eq!(
        get_device_location(
            State(state.clone()),
            Path(device.clone()),
            position_headers.clone()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );

    position_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {wrong_scope_secret}").parse().unwrap(),
    );
    assert_eq!(
        get_device_location(State(state), Path(device), position_headers)
            .await
            .err(),
        Some(StatusCode::FORBIDDEN)
    );
    let _ = active_session;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn track01_session_lifecycle_idempotency_authorization_and_concurrency() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let session_permission_function: Option<String> = sqlx::query_scalar(
        "SELECT to_regprocedure('public.gps_tracking_application_token_owner_has_session_permission(uuid,uuid,text,uuid,text)')::text",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        session_permission_function.is_some(),
        "application session authorization migration must be installed"
    );
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let auth_role = format!("track01_auth_{}", &suffix[..8]);
    let app_role = format!("track01_app_{}", &suffix[..8]);
    for (role, password, capability) in [
        (&auth_role, "track01-auth-disposable", "gps_tracking_auth"),
        (&app_role, "track01-app-disposable", "gps_tracking_app"),
    ] {
        sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD '{password}'"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(&format!("GRANT {capability} TO {role}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    let auth_pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&auth_role)
                .password("track01-auth-disposable"),
        )
        .await
        .unwrap();
    let tracking_pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE gps_tracking_app")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&app_role)
                .password("track01-app-disposable"),
        )
        .await
        .unwrap();
    let mut state = AppState::with_auth_test_key(
        auth_pool.clone(),
        "https://portal.example".into(),
        [111; 32],
    );
    state.set_tracking_pool(tracking_pool.clone());

    let tenant = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'TRACK-01 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    let admin = insert_org01_user(&pool, &format!("track01-{suffix}@example.invalid"), false).await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&admin)
        .execute(&pool).await.unwrap();
    let target_id = uuid::Uuid::new_v4().to_string();
    let other_target_id = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'TRACK-01 target'),($3::uuid,$2::uuid,'TRACK-01 second target')")
        .bind(&target_id).bind(&tenant).bind(&other_target_id)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'TRACK-01 device','http-json',$3)")
        .bind(&device_id).bind(&tenant).bind(format!("track01-{suffix}"))
        .execute(&pool).await.unwrap();
    let (cookie, csrf) = insert_org01_session(&pool, &admin, false).await;
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    headers.insert("x-csrf-token", csrf.parse().unwrap());
    headers.insert("x-organization-id", tenant.parse().unwrap());

    let start = |state: AppState, mut headers: HeaderMap, target_id: String, key: String| {
        let device_id = device_id.clone();
        async move {
            headers.insert("idempotency-key", key.parse().unwrap());
            create_tracking_session(
                State(state),
                headers,
                Json(SessionCreateRequest {
                    target_id,
                    device_id,
                }),
            )
            .await
        }
    };
    let first = start(
        state.clone(),
        headers.clone(),
        target_id.clone(),
        format!("start-{suffix}"),
    )
    .await
    .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    assert!(
        first
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("/v1/sessions/")
    );
    let first_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(first.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = first_json["id"].as_str().unwrap().to_owned();
    assert_eq!(first_json["status"], "active");
    assert_eq!(first_json["target_id"], target_id);
    assert_eq!(first_json["device_id"], device_id);
    let lifecycle_timestamps_close: bool =
        sqlx::query_scalar("SELECT abs(extract(epoch FROM ($1::timestamptz-$2::timestamptz))) < 1")
            .bind(first_json["started_at"].as_str().unwrap())
            .bind(first_json["created_at"].as_str().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(lifecycle_timestamps_close);

    let replay = start(
        state.clone(),
        headers.clone(),
        target_id.clone(),
        format!("start-{suffix}"),
    )
    .await
    .unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);
    let replay_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(replay.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay_json["id"], session_id);
    sqlx::query(
        "UPDATE memberships SET role='viewer' WHERE tenant_id=$1::uuid AND user_id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&admin)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        start(
            state.clone(),
            headers.clone(),
            target_id.clone(),
            format!("start-{suffix}"),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "idempotency replay rechecks current object permission before returning cached data"
    );
    sqlx::query("UPDATE memberships SET role='organization_admin' WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(&tenant)
        .bind(&admin)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        start(
            state.clone(),
            headers.clone(),
            other_target_id.clone(),
            format!("start-{suffix}"),
        )
        .await
        .err(),
        Some(StatusCode::CONFLICT),
        "same key with a different canonical request conflicts"
    );

    let page = list_tracking_sessions(
        State(state.clone()),
        headers.clone(),
        Query(SessionListQuery::default()),
    )
    .await
    .unwrap();
    let page_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(page.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page_json["data"].as_array().unwrap().len(), 1);
    assert_eq!(page_json["data"][0]["id"], session_id);
    let detail = get_tracking_session(
        State(state.clone()),
        Path(session_id.clone()),
        headers.clone(),
    )
    .await
    .unwrap();
    let detail_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(detail.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail_json["id"], session_id);

    let mut ingestion_tx = tracking_storage::begin_tenant_transaction(&tracking_pool, &tenant)
        .await
        .unwrap();
    tracking_storage::lock_tracking_resources(
        &mut ingestion_tx,
        &tenant,
        std::slice::from_ref(&device_id),
        std::slice::from_ref(&target_id),
        std::slice::from_ref(&session_id),
    )
    .await
    .unwrap();
    let ingestion_message_id = uuid::Uuid::new_v4().to_string();
    let insert_message_id = ingestion_message_id.clone();
    let ingestion_tenant_id = tenant.clone();
    let ingestion_device_id = device_id.clone();
    let ingestion_recorded_at = first_json["started_at"].as_str().unwrap().to_owned();
    let ingestion_request = async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        sqlx::query("INSERT INTO positions(tenant_id,device_id,message_id,payload_fingerprint,location,recorded_at) \
            VALUES($1::uuid,$2::uuid,$3::uuid,$4,ST_SetSRID(ST_MakePoint(110.84,-6.80),4326)::geography,$5::timestamptz)")
            .bind(&ingestion_tenant_id).bind(&ingestion_device_id).bind(insert_message_id).bind(vec![12u8;32]).bind(&ingestion_recorded_at)
            .execute(&mut *ingestion_tx).await.unwrap();
        ingestion_tx.commit().await.unwrap();
    };
    let end_request = end_tracking_session(
        State(state.clone()),
        Path(session_id.clone()),
        headers.clone(),
        Json(EmptySessionRequest {}),
    );
    let (ended, ()) = tokio::join!(end_request, ingestion_request);
    let ended = ended.unwrap();
    assert_eq!(ended.status(), StatusCode::OK);
    let ended_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(ended.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ended_json["status"], "ended");
    assert!(ended_json["ended_at"].as_str().unwrap() >= ended_json["started_at"].as_str().unwrap());
    let accepted_before_end: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM positions p JOIN tracking_sessions s \
         ON s.tenant_id=p.tenant_id AND s.device_id=p.device_id \
         WHERE p.tenant_id=$1::uuid AND p.device_id=$2::uuid AND p.message_id=$3::text \
           AND p.recorded_at >= s.started_at AND p.recorded_at < s.ended_at)",
    )
    .bind(&tenant)
    .bind(&device_id)
    .bind(&ingestion_message_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        accepted_before_end,
        "ingestion holding the agreed locks commits before session end"
    );
    let original_end = ended_json["ended_at"].clone();
    let ended_again = end_tracking_session(
        State(state.clone()),
        Path(session_id.clone()),
        headers.clone(),
        Json(EmptySessionRequest {}),
    )
    .await
    .unwrap();
    let ended_again_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(ended_again.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ended_again_json["ended_at"], original_end);

    let application_token_id = uuid::Uuid::new_v4().to_string();
    let application_secret = format!("gps_app_{}", random_secret());
    let application_idempotency_key = format!("app-start-{suffix}");
    let mut token_tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) VALUES($1::uuid,$2::uuid,$3::uuid,'TRACK-01 session token',$4,ARRAY['sessions:read','sessions:write'],clock_timestamp()+interval '30 days')")
        .bind(&application_token_id).bind(&tenant).bind(&admin).bind(digest(&application_secret))
        .execute(&mut *token_tx).await.unwrap();
    sqlx::query("INSERT INTO application_token_targets(tenant_id,application_token_id,target_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
        .bind(&tenant).bind(&application_token_id).bind(&target_id)
        .execute(&mut *token_tx).await.unwrap();
    sqlx::query("INSERT INTO application_token_devices(tenant_id,application_token_id,device_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
        .bind(&tenant).bind(&application_token_id).bind(&device_id)
        .execute(&mut *token_tx).await.unwrap();
    token_tx.commit().await.unwrap();
    let mut application_headers = headers.clone();
    application_headers.remove(header::ORIGIN);
    application_headers.remove(header::COOKIE);
    application_headers.remove("x-csrf-token");
    application_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {application_secret}").parse().unwrap(),
    );
    let application_session = start(
        state.clone(),
        application_headers.clone(),
        target_id.clone(),
        application_idempotency_key.clone(),
    )
    .await
    .unwrap();
    assert_eq!(application_session.status(), StatusCode::CREATED);
    let application_session_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(application_session.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let application_session_id = application_session_json["id"].as_str().unwrap().to_owned();
    let application_replay = start(
        state.clone(),
        application_headers.clone(),
        target_id.clone(),
        application_idempotency_key.clone(),
    )
    .await
    .unwrap();
    assert_eq!(application_replay.status(), StatusCode::CREATED);
    let application_replay_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(application_replay.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(application_replay_json["id"], application_session_id);
    let scoped_page = list_tracking_sessions(
        State(state.clone()),
        application_headers.clone(),
        Query(SessionListQuery {
            limit: Some(1),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let scoped_page_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(scoped_page.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(scoped_page_json["data"].as_array().unwrap().len(), 1);
    let cursor = scoped_page_json["next_cursor"].as_str().unwrap().to_owned();
    let scoped_next = list_tracking_sessions(
        State(state.clone()),
        application_headers.clone(),
        Query(SessionListQuery {
            limit: Some(1),
            cursor: Some(cursor.clone()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let scoped_next_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(scoped_next.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(scoped_next_json["data"].as_array().unwrap().len(), 1);
    let mut tampered_cursor = cursor;
    tampered_cursor.push('x');
    assert_eq!(
        list_tracking_sessions(
            State(state.clone()),
            application_headers.clone(),
            Query(SessionListQuery {
                limit: Some(1),
                cursor: Some(tampered_cursor),
                ..Default::default()
            }),
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST)
    );
    let application_ended = end_tracking_session(
        State(state.clone()),
        Path(application_session_id.clone()),
        application_headers.clone(),
        Json(EmptySessionRequest {}),
    )
    .await
    .unwrap();
    assert_eq!(application_ended.status(), StatusCode::OK);
    sqlx::query("UPDATE application_tokens SET revoked_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&application_token_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get_tracking_session(
            State(state.clone()),
            Path(application_session_id),
            application_headers.clone(),
        )
        .await
        .err(),
        Some(StatusCode::UNAUTHORIZED),
        "revoked application bearer is rejected before session access"
    );
    assert_eq!(
        start(
            state.clone(),
            application_headers,
            target_id.clone(),
            application_idempotency_key,
        )
        .await
        .err(),
        Some(StatusCode::UNAUTHORIZED),
        "cached application-token response cannot be replayed after revoke"
    );

    let missing_key = create_tracking_session(
        State(state.clone()),
        headers.clone(),
        Json(SessionCreateRequest {
            target_id: other_target_id.clone(),
            device_id: device_id.clone(),
        }),
    )
    .await;
    assert_eq!(missing_key.err(), Some(StatusCode::BAD_REQUEST));

    let first_attempt = start(
        state.clone(),
        headers.clone(),
        target_id.clone(),
        format!("parallel-a-{suffix}"),
    );
    let second_attempt = start(
        state.clone(),
        headers.clone(),
        other_target_id.clone(),
        format!("parallel-b-{suffix}"),
    );
    let (first_attempt, second_attempt) = tokio::join!(first_attempt, second_attempt);
    let first_succeeded = first_attempt
        .as_ref()
        .is_ok_and(|response| response.status() == StatusCode::CREATED);
    let second_succeeded = second_attempt
        .as_ref()
        .is_ok_and(|response| response.status() == StatusCode::CREATED);
    assert_ne!(first_succeeded, second_succeeded);
    assert!(
        matches!(
            if first_succeeded {
                &second_attempt
            } else {
                &first_attempt
            },
            Err(StatusCode::CONFLICT)
        ),
        "the competing active device/target session conflicts"
    );
    let winner_response = if first_succeeded {
        first_attempt.unwrap()
    } else {
        second_attempt.unwrap()
    };
    let winner_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(winner_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let winner_id = winner_json["id"].as_str().unwrap().to_owned();
    let first_end = end_tracking_session(
        State(state.clone()),
        Path(winner_id.clone()),
        headers.clone(),
        Json(EmptySessionRequest {}),
    );
    let second_end = end_tracking_session(
        State(state.clone()),
        Path(winner_id),
        headers.clone(),
        Json(EmptySessionRequest {}),
    );
    let (first_end, second_end) = tokio::join!(first_end, second_end);
    let first_end_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(first_end.unwrap().into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let second_end_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(second_end.unwrap().into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first_end_json["ended_at"], second_end_json["ended_at"]);

    let forbidden_user = insert_org01_user(
        &pool,
        &format!("track01-viewer-{suffix}@example.invalid"),
        false,
    )
    .await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'viewer','active')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&forbidden_user)
        .execute(&pool).await.unwrap();
    let (viewer_cookie, viewer_csrf) = insert_org01_session(&pool, &forbidden_user, false).await;
    let mut viewer_headers = headers.clone();
    viewer_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={viewer_cookie}")
            .parse()
            .unwrap(),
    );
    viewer_headers.insert("x-csrf-token", viewer_csrf.parse().unwrap());
    assert_eq!(
        end_tracking_session(
            State(state.clone()),
            Path(session_id.clone()),
            viewer_headers,
            Json(EmptySessionRequest {}),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "viewer without an object grant cannot end a session"
    );

    auth_pool.close().await;
    tracking_pool.close().await;
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn track02_http_ingestion_canonical_dedupe_policy_and_atomicity() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let message_id_type: String = sqlx::query_scalar("SELECT data_type FROM information_schema.columns WHERE table_schema='public' AND table_name='positions' AND column_name='message_id'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        message_id_type, "text",
        "opaque HTTP message ID migration must be applied"
    );
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("track02_app_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'track02-disposable'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT gps_tracking_app TO {role}"))
        .execute(&pool)
        .await
        .unwrap();
    let tracking_pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE gps_tracking_app")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&role)
                .password("track02-disposable"),
        )
        .await
        .unwrap();
    let mut state = AppState::new();
    state.set_tracking_pool(tracking_pool.clone());

    let tenant = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();
    let other_device_id = uuid::Uuid::new_v4().to_string();
    let session_device_id = uuid::Uuid::new_v4().to_string();
    let secret = format!("gps_live_track02_{suffix}");
    let second_secret = format!("gps_live_track02_other_{suffix}");
    let session_secret = format!("gps_live_track02_session_{suffix}");
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'TRACK-02 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    for (id, label, policy) in [
        (&device_id, "primary", "always"),
        (&other_device_id, "other", "always"),
        (&session_device_id, "session", "session_only"),
    ] {
        sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier,ingestion_policy) VALUES($1::uuid,$2::uuid,$3,'http-json',$4,$5)")
            .bind(id).bind(&tenant).bind(format!("TRACK-02 {label}"))
            .bind(format!("track02-{label}-{suffix}")).bind(policy)
            .execute(&pool).await.unwrap();
    }
    for (device, token) in [
        (&device_id, &secret),
        (&other_device_id, &second_secret),
        (&session_device_id, &session_secret),
    ] {
        sqlx::query("INSERT INTO device_credentials(id,tenant_id,device_id,token_hash) VALUES($1::uuid,$2::uuid,$3::uuid,$4)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(device)
            .bind(digest(token)).execute(&pool).await.unwrap();
    }

    let (utc_recorded_at, jakarta_recorded_at): (String, String) = sqlx::query_as("WITH stamp AS (SELECT clock_timestamp()-interval '5 seconds' AS value) SELECT to_char(value AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), to_char(value AT TIME ZONE 'Asia/Jakarta','YYYY-MM-DD\"T\"HH24:MI:SS.US\"+07:00') FROM stamp")
        .fetch_one(&pool).await.unwrap();
    let make_request =
        |message_id: &str, latitude: f64, attributes: serde_json::Value, recorded_at: &str| {
            serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
                "device_id": device_id,
                "points": [{
                    "message_id": message_id,
                    "latitude": latitude,
                    "longitude": 110.84,
                    "recorded_at": recorded_at,
                    "attributes": attributes
                }]
            }))
            .unwrap()
        };
    let invoke = |token: String, request: PositionBatchRequest, app: AppState| {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        async move { ingest_positions(State(app), headers, Json(request)).await }
    };
    let message_id = format!("opaque-message-{suffix}");
    let first = invoke(
        secret.clone(),
        make_request(
            &message_id,
            -6.8,
            serde_json::json!({"b":2,"a":1}),
            &jakarta_recorded_at,
        ),
        state.clone(),
    )
    .await
    .unwrap()
    .into_response();
    assert_eq!(first.status(), StatusCode::OK);
    let first_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(first.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first_body["summary"]["stored"], 1);

    let mixed_future_at: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()+interval '3 minutes') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&pool).await.unwrap();
    let mixed = serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
        "device_id": device_id,
        "points": [
          {"message_id":message_id,"latitude":-6.8,"longitude":110.84,"recorded_at":jakarta_recorded_at,"attributes":{"a":1,"b":2}},
          {"message_id":format!("mixed-stored-{suffix}"),"latitude":-6.81,"longitude":110.84,"recorded_at":utc_recorded_at},
          {"message_id":format!("mixed-rejected-{suffix}"),"latitude":-6.82,"longitude":110.84,"recorded_at":mixed_future_at}
        ]
    })).unwrap();
    let mixed = invoke(secret.clone(), mixed, state.clone())
        .await
        .unwrap()
        .into_response();
    let mixed_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(mixed.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(mixed_body["summary"]["stored"], 1);
    assert_eq!(mixed_body["summary"]["duplicate"], 1);
    assert_eq!(mixed_body["summary"]["rejected"], 1);
    assert_eq!(mixed_body["results"][2]["reason"], "TIMESTAMP_IN_FUTURE");

    let invalid_measurement = serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
        "device_id": device_id,
        "points": [{"message_id":"invalid-heading","latitude":-6.8,"longitude":110.84,"heading_deg":360,"recorded_at":utc_recorded_at}]
    })).unwrap();
    assert!(matches!(
        invoke(secret.clone(), invalid_measurement, state.clone()).await,
        Err(StatusCode::UNPROCESSABLE_ENTITY)
    ));

    let same = invoke(
        secret.clone(),
        make_request(
            &message_id,
            -6.8,
            serde_json::json!({"a":1,"b":2}),
            &utc_recorded_at,
        ),
        state.clone(),
    )
    .await
    .unwrap()
    .into_response();
    let same_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(same.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        same_body["summary"]["duplicate"], 1,
        "attribute key order and timestamp offset normalize canonically"
    );
    let conflict = invoke(
        secret.clone(),
        make_request(
            &message_id,
            -6.7,
            serde_json::json!({"a":1,"b":2}),
            &utc_recorded_at,
        ),
        state.clone(),
    )
    .await
    .unwrap()
    .into_response();
    let conflict_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(conflict.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(conflict_body["results"][0]["reason"], "MESSAGE_ID_CONFLICT");

    let wrong_device = serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
        "device_id": other_device_id,
        "points": [{"message_id":"wrong-device","latitude":-6.8,"longitude":110.84,"recorded_at":"2026-10-19T03:00:10Z"}]
    })).unwrap();
    assert!(matches!(
        invoke(secret.clone(), wrong_device, state.clone()).await,
        Err(StatusCode::FORBIDDEN)
    ));

    let racing_id = format!("race-{suffix}");
    let left = invoke(
        secret.clone(),
        make_request(&racing_id, -6.81, serde_json::json!({}), &utc_recorded_at),
        state.clone(),
    );
    let right = invoke(
        secret.clone(),
        make_request(&racing_id, -6.81, serde_json::json!({}), &utc_recorded_at),
        state.clone(),
    );
    let (left, right) = tokio::join!(left, right);
    let summaries = [left.unwrap(), right.unwrap()]
        .into_iter()
        .map(|response| response.into_response());
    let mut states = Vec::new();
    for response in summaries {
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        states.push(if body["summary"]["stored"] == 1 {
            "stored"
        } else {
            "duplicate"
        });
    }
    states.sort_unstable();
    assert_eq!(states, ["duplicate", "stored"]);

    let session_token = |message_id: &str, recorded_at: String| {
        serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
            "device_id": session_device_id,
            "points": [{"message_id":message_id,"latitude":-6.82,"longitude":110.84,"recorded_at":recorded_at}]
        })).unwrap()
    };
    let future_at: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()+interval '15 seconds') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&pool).await.unwrap();
    let future = invoke(
        session_secret.clone(),
        session_token("not-ready-yet", future_at),
        state.clone(),
    )
    .await
    .unwrap()
    .into_response();
    let future_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(future.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(future_body["results"][0]["reason"], "TIMESTAMP_NOT_READY");
    assert_eq!(future_body["results"][0]["retryable"], true);
    assert!(
        future_body["results"][0]["retry_after_ms"]
            .as_i64()
            .unwrap()
            > 0
    );

    let target_id = uuid::Uuid::new_v4().to_string();
    let historical_session_id = uuid::Uuid::new_v4().to_string();
    let (late_recorded_at, started_at, ended_at): (String, String, String) = sqlx::query_as("WITH bounds AS (SELECT clock_timestamp()-interval '30 minutes' AS started, clock_timestamp()-interval '10 minutes' AS ended, clock_timestamp()-interval '20 minutes' AS recorded) SELECT to_char(recorded AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), to_char(started AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), to_char(ended AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') FROM bounds")
        .fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'TRACK-02 historical target')")
        .bind(&target_id).bind(&tenant).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO tracking_sessions(id,tenant_id,target_id,device_id,started_at,ended_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::timestamptz,$6::timestamptz)")
        .bind(&historical_session_id).bind(&tenant).bind(&target_id).bind(&session_device_id).bind(&started_at).bind(&ended_at)
        .execute(&pool).await.unwrap();
    let late_request = session_token("late-session-point", late_recorded_at);
    let late = invoke(session_secret.clone(), late_request, state.clone())
        .await
        .unwrap()
        .into_response();
    let late_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(late.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        late_body["summary"]["stored"], 1,
        "valid late points in a closed session remain admissible"
    );
    let exact_end = invoke(
        session_secret.clone(),
        session_token("at-session-end", ended_at.clone()),
        state.clone(),
    )
    .await
    .unwrap()
    .into_response();
    let exact_end_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(exact_end.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        exact_end_body["results"][0]["reason"], "OUTSIDE_SESSION",
        "session end is exclusive"
    );
    let outside_at: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()-interval '2 seconds') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&pool).await.unwrap();
    let outside = invoke(
        session_secret,
        session_token("outside-every-session", outside_at),
        state.clone(),
    )
    .await
    .unwrap()
    .into_response();
    let outside_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(outside.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(outside_body["results"][0]["reason"], "OUTSIDE_SESSION");

    // A database error on a later item must roll back the earlier insert in this batch.
    let failing_id = format!("fail-{suffix}");
    let function = format!("track02_fail_{}", &suffix[..8]);
    let trigger = format!("track02_trigger_{}", &suffix[..8]);
    sqlx::query(&format!("CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.message_id = '{failing_id}-second' THEN RAISE EXCEPTION 'synthetic database failure'; END IF; RETURN NEW; END $$"))
        .execute(&pool).await.unwrap();
    sqlx::query(&format!("CREATE TRIGGER {trigger} BEFORE INSERT ON positions FOR EACH ROW EXECUTE FUNCTION {function}()"))
        .execute(&pool).await.unwrap();
    let batch = serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
        "device_id": device_id,
        "points": [
          {"message_id":format!("{failing_id}-first"),"latitude":-6.8,"longitude":110.84,"recorded_at":utc_recorded_at},
          {"message_id":format!("{failing_id}-second"),"latitude":-6.8,"longitude":110.84,"recorded_at":utc_recorded_at}
        ]
    })).unwrap();
    assert!(matches!(
        invoke(secret, batch, state).await,
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM positions WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND message_id LIKE $3")
        .bind(&tenant).bind(&device_id).bind(format!("{failing_id}-%"))
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
    sqlx::query(&format!("DROP TRIGGER {trigger} ON positions"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP FUNCTION {function}()"))
        .execute(&pool)
        .await
        .unwrap();

    tracking_pool.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
}
