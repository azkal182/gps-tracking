use super::*;
use axum::extract::Path;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
async fn ops02_runtime_pool_sets_statement_timeout() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = database_pool_options()
        .connect(&database_url)
        .await
        .unwrap();
    let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(timeout, "10s");
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn live03_snapshot_race_duplicate_backfill_and_target_reassignment() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("live03_app_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'live03-disposable'"
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
                .password("live03-disposable"),
        )
        .await
        .unwrap();
    let mut state =
        AppState::with_auth_test_key(pool.clone(), "https://portal.example".into(), [115; 32]);
    state.set_tracking_pool(tracking_pool.clone());

    let tenant = uuid::Uuid::new_v4().to_string();
    let target_a = uuid::Uuid::new_v4().to_string();
    let target_b = uuid::Uuid::new_v4().to_string();
    let device = uuid::Uuid::new_v4().to_string();
    let session_a = uuid::Uuid::new_v4().to_string();
    let secret = format!("gps_live_live03_{suffix}");
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'LIVE-03 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    let user = insert_org01_user(&pool, &format!("live03-{suffix}@example.invalid"), false).await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&tenant)
        .bind(&user)
        .execute(&pool)
        .await
        .unwrap();
    for (id, name) in [
        (&target_a, "LIVE-03 target A"),
        (&target_b, "LIVE-03 target B"),
    ] {
        sqlx::query("INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,$3)")
            .bind(id)
            .bind(&tenant)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier,ingestion_policy) VALUES($1::uuid,$2::uuid,'LIVE-03 device','http-json',$3,'session_only')")
        .bind(&device)
        .bind(&tenant)
        .bind(format!("live03-{suffix}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO device_credentials(id,tenant_id,device_id,token_hash) VALUES($1::uuid,$2::uuid,$3::uuid,$4)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&tenant)
        .bind(&device)
        .bind(digest(&secret))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tracking_sessions(id,tenant_id,target_id,device_id,started_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,clock_timestamp()-interval '10 minutes')")
        .bind(&session_a)
        .bind(&tenant)
        .bind(&target_a)
        .bind(&device)
        .execute(&pool)
        .await
        .unwrap();
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

    let ticket_response = create_realtime_ticket(
        State(state.clone()),
        portal_headers.clone(),
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let ticket: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(ticket_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let test_state = state.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let mut request = format!(
        "ws://{address}/v1/ws/tracking?ticket={}",
        ticket["ticket"].as_str().unwrap()
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({"type":"subscribe","request_id":"live03-sub","target_ids":[target_a,target_b],"device_ids":[device]})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let ack = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(ack.to_text().unwrap().contains("subscribed"));

    let target_ticket_response = create_realtime_ticket(
        State(test_state.clone()),
        portal_headers.clone(),
        Json(EmptyTicketRequest {}),
    )
    .await
    .unwrap();
    let target_ticket: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(target_ticket_response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    let mut target_request = format!(
        "ws://{address}/v1/ws/tracking?ticket={}",
        target_ticket["ticket"].as_str().unwrap()
    )
    .into_client_request()
    .unwrap();
    target_request
        .headers_mut()
        .insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    let (mut target_socket, _) = tokio_tungstenite::connect_async(target_request)
        .await
        .unwrap();
    target_socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({"type":"subscribe","request_id":"live03-target-sub","target_ids":[target_a],"device_ids":[]})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let target_ack = tokio::time::timeout(std::time::Duration::from_secs(2), target_socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(target_ack.to_text().unwrap().contains("subscribed"));

    let first_time: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()-interval '3 minutes') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&pool)
        .await
        .unwrap();
    let make_batch = |message_id: &str, recorded_at: &str| {
        serde_json::from_value::<PositionBatchRequest>(serde_json::json!({
            "device_id": device,
            "points": [{"message_id":message_id,"latitude":-6.81,"longitude":110.84,"recorded_at":recorded_at}]
        }))
        .unwrap()
    };
    let first_response = ingest_positions(
        State(test_state.clone()),
        device_headers.clone(),
        Json(make_batch("live03-first", &first_time)),
    )
    .await
    .unwrap()
    .into_response();
    let first_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(first_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first_json["results"][0]["status"], "stored");
    let event_a = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let event_a: serde_json::Value = serde_json::from_str(event_a.to_text().unwrap()).unwrap();
    let first_position_id = first_json["results"][0]["position_id"].as_str().unwrap();
    assert_eq!(event_a["type"], "position.updated");
    assert_eq!(event_a["data"]["target_id"], target_a);
    assert_eq!(event_a["data"]["session_id"], session_a);
    assert_eq!(event_a["data"]["position"]["id"], first_position_id);
    assert!(first_position_id.parse::<i64>().is_ok());
    assert!(
        event_a["data"]["position"]["recorded_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    let target_event_a =
        tokio::time::timeout(std::time::Duration::from_secs(2), target_socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    assert!(
        target_event_a
            .to_text()
            .unwrap()
            .contains("position.updated")
    );
    let committed_before_broadcast: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM positions WHERE tenant_id=$1::uuid AND id=$2::bigint)",
    )
    .bind(&tenant)
    .bind(first_position_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(committed_before_broadcast);

    let snapshot_a = get_device_location(
        State(test_state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let snapshot_a: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(snapshot_a.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(snapshot_a["observed_at"].is_string());
    assert_eq!(snapshot_a["tenant_id"], tenant);
    assert_eq!(snapshot_a["position"]["id"], first_position_id);

    let duplicate = ingest_positions(
        State(test_state.clone()),
        device_headers.clone(),
        Json(make_batch("live03-first", &first_time)),
    )
    .await
    .unwrap()
    .into_response();
    let duplicate: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(duplicate.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(duplicate["results"][0]["status"], "duplicate");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), socket.next())
            .await
            .is_err(),
        "duplicate must not publish another position event"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), target_socket.next())
            .await
            .is_err()
    );

    let backfill_time: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()-interval '4 minutes') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&pool)
        .await
        .unwrap();
    let backfill = ingest_positions(
        State(test_state.clone()),
        device_headers.clone(),
        Json(make_batch("live03-backfill", &backfill_time)),
    )
    .await
    .unwrap()
    .into_response();
    let backfill: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(backfill.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(backfill["results"][0]["status"], "stored");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), socket.next())
            .await
            .is_err(),
        "older backfill must not replace latest or emit position.updated"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), target_socket.next())
            .await
            .is_err()
    );

    let ended = end_tracking_session(
        State(test_state.clone()),
        Path(session_a.clone()),
        portal_headers.clone(),
        Json(EmptySessionRequest {}),
    )
    .await
    .unwrap();
    let ended: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(ended.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(ended["ended_at"].is_string());
    let end_event = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(end_event.to_text().unwrap().contains("session.ended"));
    let target_end_event =
        tokio::time::timeout(std::time::Duration::from_secs(2), target_socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    assert!(
        target_end_event
            .to_text()
            .unwrap()
            .contains("session.ended")
    );
    let ended_before_broadcast: bool = sqlx::query_scalar(
        "SELECT ended_at IS NOT NULL FROM tracking_sessions WHERE tenant_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&session_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(ended_before_broadcast);

    let mut session_headers = portal_headers.clone();
    session_headers.insert("idempotency-key", "live03-reassignment".parse().unwrap());
    let created = create_tracking_session(
        State(test_state.clone()),
        session_headers,
        Json(SessionCreateRequest {
            target_id: target_b.clone(),
            device_id: device.clone(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(created.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_b = created["id"].as_str().unwrap();
    let started_before_broadcast: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tracking_sessions WHERE tenant_id=$1::uuid AND id=$2::uuid AND target_id=$3::uuid AND ended_at IS NULL)",
    )
    .bind(&tenant)
    .bind(session_b)
    .bind(&target_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(started_before_broadcast);
    let start_event = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let start_event: serde_json::Value =
        serde_json::from_str(start_event.to_text().unwrap()).unwrap();
    assert_eq!(start_event["type"], "session.started");
    assert_eq!(start_event["data"]["session_id"], session_b);

    let reassigned_backfill_time: String = sqlx::query_scalar("SELECT to_char((clock_timestamp()-interval '2 minutes') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')")
        .fetch_one(&pool)
        .await
        .unwrap();
    publish_event(
        &test_state,
        &tenant,
        Some(&device),
        Some(&target_a),
        Some(&session_a),
        "position.updated",
        &reassigned_backfill_time,
        serde_json::json!({
            "device_id":device,
            "target_id":target_a,
            "session_id":session_a,
            "position":{"recorded_at":reassigned_backfill_time}
        }),
    );
    let device_projection = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        device_projection
            .to_text()
            .unwrap()
            .contains("position.updated")
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), target_socket.next())
            .await
            .is_err(),
        "worker/ingestion race must suppress a projection for an ended target session"
    );
    let reassigned = ingest_positions(
        State(test_state.clone()),
        device_headers,
        Json(make_batch(
            "live03-old-session-late",
            &reassigned_backfill_time,
        )),
    )
    .await
    .unwrap()
    .into_response();
    let reassigned: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(reassigned.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reassigned["results"][0]["status"], "stored");
    let event_b = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let event_b: serde_json::Value = serde_json::from_str(event_b.to_text().unwrap()).unwrap();
    assert_eq!(event_b["type"], "position.updated");
    assert_eq!(event_b["data"]["target_id"], serde_json::Value::Null);
    assert_eq!(event_b["data"]["session_id"], serde_json::Value::Null);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), target_socket.next())
            .await
            .is_err(),
        "a late point belonging to an ended target session must not reach that target subscriber"
    );
    let event_position = &event_b["data"]["position"];
    assert_eq!(
        event_position["id"],
        reassigned["results"][0]["position_id"]
    );
    let snapshot_b = get_device_location(
        State(test_state.clone()),
        Path(device.clone()),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let snapshot_b: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(snapshot_b.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot_b["position"]["id"], event_position["id"]);
    let old_recorded_at = snapshot_a["position"]["recorded_at"].as_str().unwrap();
    let new_recorded_at = event_position["recorded_at"].as_str().unwrap();
    assert!(
        new_recorded_at > old_recorded_at,
        "stale snapshot captured before event cannot overwrite a newer position when the consumer orders by (recorded_at, id): old={old_recorded_at}, new={new_recorded_at}, old_id={}, new_id={}",
        snapshot_a["position"]["id"],
        event_position["id"]
    );
    assert!(snapshot_b["observed_at"].as_str().unwrap() >= event_b["emitted_at"].as_str().unwrap());

    let target_a_snapshot = get_target_location(
        State(test_state.clone()),
        Path(target_a),
        portal_headers.clone(),
    )
    .await
    .unwrap();
    let target_a_snapshot: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(target_a_snapshot.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(target_a_snapshot["tracking_status"], "inactive");
    assert_eq!(target_a_snapshot["position"], serde_json::Value::Null);
    let target_b_snapshot = get_target_location(State(test_state), Path(target_b), portal_headers)
        .await
        .unwrap();
    let target_b_snapshot: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(target_b_snapshot.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(target_b_snapshot["tracking_status"], "active");
    assert_eq!(target_b_snapshot["position"], serde_json::Value::Null);

    socket.close(None).await.unwrap();
    target_socket.close(None).await.unwrap();
    server.abort();
    tracking_pool.close().await;
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
}
