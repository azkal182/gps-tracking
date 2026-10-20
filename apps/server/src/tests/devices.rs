use super::*;

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn device01_target_device_routes_enforce_grants_and_validation() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let auth_role = format!("device01_auth_{}", &suffix[..8]);
    let tracking_role = format!("device01_app_{}", &suffix[..8]);
    for (role, password, capability) in [
        (&auth_role, "device01-auth-disposable", "gps_tracking_auth"),
        (
            &tracking_role,
            "device01-app-disposable",
            "gps_tracking_app",
        ),
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
                .password("device01-auth-disposable"),
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
                .username(&tracking_role)
                .password("device01-app-disposable"),
        )
        .await
        .unwrap();
    let mut state =
        AppState::with_auth_test_key(auth_pool.clone(), "https://portal.example".into(), [91; 32]);
    state.set_tracking_pool(tracking_pool.clone());
    let user = insert_org01_user(&pool, &format!("device01-{suffix}@example.invalid"), false).await;
    let tenant = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'DEVICE-01 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&user).execute(&pool).await.unwrap();
    let (cookie, csrf) = insert_org01_session(&pool, &user, false).await;
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    headers.insert("x-csrf-token", csrf.parse().unwrap());
    headers.insert("x-organization-id", tenant.parse().unwrap());

    let created_target = create_target(State(state.clone()), headers.clone(), Json(serde_json::json!({"name":"  Asset A  ","external_ref":"asset-a","metadata":{"fleet":"north"}}))).await.unwrap();
    assert_eq!(created_target.0, StatusCode::CREATED);
    assert_eq!(created_target.2.0.name, "Asset A");
    let target_id = created_target.2.0.id.clone();
    assert_eq!(
        get_target(
            State(state.clone()),
            headers.clone(),
            axum::extract::Path("not-a-uuid".into())
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST)
    );
    assert!(
        require_object_permission(&auth_pool, &tenant, &user, "target", &target_id, "write")
            .await
            .is_ok(),
        "creator receives read/write grants"
    );
    let foreign_tenant = uuid::Uuid::new_v4().to_string();
    let foreign_target = uuid::Uuid::new_v4().to_string();
    let foreign_device = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'DEVICE-01 foreign tenant')")
        .bind(&foreign_tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'DEVICE-01 foreign target')")
        .bind(&foreign_target).bind(&foreign_tenant).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'DEVICE-01 foreign device','http-json',$3)")
        .bind(&foreign_device).bind(&foreign_tenant).bind(format!("device01-foreign-{suffix}" )).execute(&pool).await.unwrap();
    assert_eq!(
        create_target(
            State(state.clone()),
            headers.clone(),
            Json(serde_json::json!({"name":"duplicate ref","external_ref":"asset-a"}))
        )
        .await
        .err(),
        Some(StatusCode::CONFLICT)
    );
    assert_eq!(
        create_target(
            State(state.clone()),
            headers.clone(),
            Json(serde_json::json!({"name":"   "}))
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );
    assert_eq!(
        create_target(
            State(state.clone()),
            headers.clone(),
            Json(serde_json::json!({"name":"bad metadata","metadata":[]}))
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );
    assert_eq!(
        create_target(
            State(state.clone()),
            headers.clone(),
            Json(serde_json::json!({"name":"unknown","enabled":true}))
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );
    let target_page = list_targets(
        State(state.clone()),
        headers.clone(),
        Query(ResourceQuery::default()),
    )
    .await
    .unwrap();
    assert_eq!(target_page.0.data.len(), 1);
    assert_eq!(target_page.0.data[0].tenant_id, tenant);
    assert_eq!(
        get_target_location(
            State(state.clone()),
            axum::extract::Path(foreign_target.clone()),
            headers.clone(),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "an organization admin cannot resolve a foreign target as an inactive local target"
    );
    assert_eq!(
        get_target(
            State(state.clone()),
            headers.clone(),
            axum::extract::Path(target_id.clone())
        )
        .await
        .unwrap()
        .0
        .external_ref
        .as_deref(),
        Some("asset-a")
    );
    let updated_target = update_target(
        State(state.clone()),
        headers.clone(),
        axum::extract::Path(target_id.clone()),
        Json(serde_json::json!({"external_ref":null,"name":"Asset A Updated"})),
    )
    .await
    .unwrap();
    assert_eq!(updated_target.0.name, "Asset A Updated");
    assert_eq!(updated_target.0.external_ref, None);
    assert_eq!(
        update_target(
            State(state.clone()),
            headers.clone(),
            axum::extract::Path(target_id.clone()),
            Json(serde_json::json!({"tenant_id":uuid::Uuid::new_v4().to_string()}))
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );

    let created_device = create_device(
        State(state.clone()),
        headers.clone(),
        Json(
            serde_json::json!({"name":"Tracker A","protocol":"http-json","identifier":"serial-a"}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(created_device.0, StatusCode::CREATED);
    let device_id = created_device.2.0.id.clone();
    assert_eq!(create_device(State(state.clone()),headers.clone(),Json(serde_json::json!({"name":"duplicate","protocol":"http-json","identifier":"serial-a"}))).await.err(),Some(StatusCode::CONFLICT));
    assert_eq!(
        create_device(
            State(state.clone()),
            headers.clone(),
            Json(
                serde_json::json!({"name":"unsupported","protocol":"gt06","identifier":"serial-x"})
            )
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );
    let device_page = list_devices(
        State(state.clone()),
        headers.clone(),
        Query(ResourceQuery {
            protocol: Some("http-json".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(device_page.0.data.len(), 1);
    assert_eq!(device_page.0.data[0].tenant_id, tenant);

    // Even if a development database URL accidentally bypasses RLS, explicit
    // tenant predicates and resource ownership checks keep the API scoped.
    let mut bypass_state =
        AppState::with_auth_test_key(auth_pool.clone(), "https://portal.example".into(), [91; 32]);
    bypass_state.set_tracking_pool(pool.clone());
    let bypass_targets = list_targets(
        State(bypass_state.clone()),
        headers.clone(),
        Query(ResourceQuery::default()),
    )
    .await
    .unwrap();
    let bypass_devices = list_devices(
        State(bypass_state.clone()),
        headers.clone(),
        Query(ResourceQuery::default()),
    )
    .await
    .unwrap();
    assert!(
        bypass_targets
            .0
            .data
            .iter()
            .all(|row| row.tenant_id == tenant)
    );
    assert!(
        bypass_devices
            .0
            .data
            .iter()
            .all(|row| row.tenant_id == tenant)
    );
    assert_eq!(
        get_target_location(
            State(bypass_state),
            axum::extract::Path(foreign_target),
            headers.clone(),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        get_device(
            State(state.clone()),
            headers.clone(),
            axum::extract::Path(device_id.clone())
        )
        .await
        .unwrap()
        .0
        .identifier,
        "serial-a"
    );
    assert!(
        !update_device(
            State(state.clone()),
            headers.clone(),
            axum::extract::Path(device_id.clone()),
            Json(serde_json::json!({"enabled":false,"ingestion_policy":"always"}))
        )
        .await
        .unwrap()
        .0
        .enabled
    );
    assert_eq!(
        update_device(
            State(state.clone()),
            headers.clone(),
            axum::extract::Path(device_id.clone()),
            Json(serde_json::json!({"identifier":"changed"}))
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );

    let viewer = insert_org01_user(
        &pool,
        &format!("device01-viewer-{suffix}@example.invalid"),
        false,
    )
    .await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'viewer','active')").bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&viewer).execute(&pool).await.unwrap();
    let (viewer_cookie, viewer_csrf) = insert_org01_session(&pool, &viewer, false).await;
    let mut viewer_headers = headers.clone();
    viewer_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={viewer_cookie}")
            .parse()
            .unwrap(),
    );
    viewer_headers.insert("x-csrf-token", viewer_csrf.parse().unwrap());
    assert!(
        list_targets(
            State(state.clone()),
            viewer_headers.clone(),
            Query(ResourceQuery::default())
        )
        .await
        .unwrap()
        .0
        .data
        .is_empty(),
        "list omits resources without grants"
    );
    assert_eq!(
        get_target(
            State(state.clone()),
            viewer_headers.clone(),
            axum::extract::Path(target_id.clone())
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "object read without grant is denied"
    );
    assert_eq!(
        update_target(
            State(state.clone()),
            viewer_headers,
            axum::extract::Path(target_id.clone()),
            Json(serde_json::json!({"name":"unauthorized"}))
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "object write without grant is denied"
    );
    let mut cross_tenant = headers.clone();
    cross_tenant.insert(
        "x-organization-id",
        uuid::Uuid::new_v4().to_string().parse().unwrap(),
    );
    assert_eq!(
        get_target(
            State(state.clone()),
            cross_tenant,
            axum::extract::Path(target_id)
        )
        .await
        .err(),
        Some(StatusCode::NOT_FOUND),
        "RLS prevents cross-tenant read"
    );
    auth_pool.close().await;
    tracking_pool.close().await;
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn device02_credential_lifecycle_permissions_and_device_locking() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let auth_role = format!("device02_auth_{}", &suffix[..8]);
    let app_role = format!("device02_app_{}", &suffix[..8]);
    for (role, password, capability) in [
        (&auth_role, "device02-auth-disposable", "gps_tracking_auth"),
        (&app_role, "device02-app-disposable", "gps_tracking_app"),
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
                .password("device02-auth-disposable"),
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
                .password("device02-app-disposable"),
        )
        .await
        .unwrap();
    let mut state =
        AppState::with_auth_test_key(auth_pool.clone(), "https://portal.example".into(), [92; 32]);
    state.set_tracking_pool(tracking_pool.clone());
    let tenant = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'DEVICE-02 fixture')")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    let admin = insert_org01_user(
        &pool,
        &format!("device02-admin-{suffix}@example.invalid"),
        false,
    )
    .await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&admin).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&admin).bind(vec![7u8;32]).bind(vec![8u8;12]).execute(&pool).await.unwrap();
    let (cookie, csrf) = insert_org01_session(&pool, &admin, true).await;
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    headers.insert("x-csrf-token", csrf.parse().unwrap());
    headers.insert("x-organization-id", tenant.parse().unwrap());
    let device = create_device(
            State(state.clone()),
            headers.clone(),
            Json(serde_json::json!({"name":"DEVICE-02 fixture","protocol":"http-json","identifier":"device02-serial"})),
        )
        .await
        .unwrap()
        .2
        .0;
    let device_id = device.id.clone();

    let rotate = |state: AppState, headers: HeaderMap, id: String| async move {
        rotate_device_credential(
            State(state),
            headers,
            axum::extract::Path(id),
            Json(EmptyRequest {}),
        )
        .await
    };
    let (stale_cookie, stale_csrf) = insert_org01_session(&pool, &admin, false).await;
    let mut stale_headers = headers.clone();
    stale_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={stale_cookie}")
            .parse()
            .unwrap(),
    );
    stale_headers.insert("x-csrf-token", stale_csrf.parse().unwrap());
    assert_eq!(
        rotate(state.clone(), stale_headers, device_id.clone())
            .await
            .err(),
        Some(StatusCode::FORBIDDEN),
        "credential rotation requires recent MFA-backed authentication"
    );
    let first_response = rotate(state.clone(), headers.clone(), device_id.clone())
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);
    assert_eq!(
        first_response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let first_body = axum::body::to_bytes(first_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let first_json: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
    let first_token = first_json["token"].as_str().unwrap().to_owned();
    let first_credential =
        tracking_storage::lookup_device_credential(&tracking_pool, &digest(&first_token))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(first_credential.device_id, device_id);
    assert!(!first_json["token"].as_str().unwrap().is_empty());
    let first_hash_matches: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM device_credentials WHERE device_id=$1::uuid AND token_hash=$2)",
        )
        .bind(&device_id)
        .bind(digest(&first_token))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(first_hash_matches, "only the digest is stored");

    let credential_page = list_device_credentials(
        State(state.clone()),
        headers.clone(),
        axum::extract::Path(device_id.clone()),
        Query(PageQuery::default()),
    )
    .await
    .unwrap();
    assert_eq!(
        credential_page
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap(),
        "no-store"
    );
    let credential_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(credential_page.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(credential_json["data"].as_array().unwrap().len(), 1);
    let metadata_text = credential_json.to_string();
    assert!(!metadata_text.contains(&first_token));

    let second_response = rotate(state.clone(), headers.clone(), device_id.clone())
        .await
        .unwrap();
    let second_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(second_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let second_token = second_json["token"].as_str().unwrap().to_owned();
    assert!(
        tracking_storage::lookup_device_credential(&tracking_pool, &digest(&first_token))
            .await
            .unwrap()
            .is_none(),
        "rotating invalidates the previous secret"
    );

    let mut ingestion_tx = tracking_storage::begin_tenant_transaction(&tracking_pool, &tenant)
        .await
        .unwrap();
    tracking_storage::lock_tracking_resources(
        &mut ingestion_tx,
        &tenant,
        std::slice::from_ref(&device_id),
        &[],
        &[],
    )
    .await
    .unwrap();
    let mut rotation_task = tokio::spawn(rotate(state.clone(), headers.clone(), device_id.clone()));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), &mut rotation_task)
            .await
            .is_err(),
        "rotation waits for the shared device lock used by ingestion"
    );
    ingestion_tx.commit().await.unwrap();
    let concurrent_rotation = rotation_task.await.unwrap().unwrap();
    assert_eq!(concurrent_rotation.status(), StatusCode::OK);
    let third_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(concurrent_rotation.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let third_token = third_json["token"].as_str().unwrap().to_owned();
    assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM device_credentials WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND revoked_at IS NULL")
                .bind(&tenant).bind(&device_id).fetch_one(&pool).await.unwrap(),
            1,
            "rotation leaves exactly one active credential"
        );
    assert!(
        tracking_storage::lookup_device_credential(&tracking_pool, &digest(&second_token))
            .await
            .unwrap()
            .is_none(),
        "the serialized rotation revokes the previous token"
    );

    let revoked_id = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM device_credentials WHERE device_id=$1::uuid AND token_hash=$2",
    )
    .bind(&device_id)
    .bind(digest(&third_token))
    .fetch_one(&pool)
    .await
    .unwrap();
    let delete = revoke_device_credential(
        State(state.clone()),
        headers.clone(),
        axum::extract::Path((device_id.clone(), revoked_id)),
    )
    .await
    .unwrap();
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);
    assert!(
        tracking_storage::lookup_device_credential(&tracking_pool, &digest(&third_token))
            .await
            .unwrap()
            .is_none(),
        "explicit revoke rejects the previously active credential"
    );

    let operator = insert_org01_user(
        &pool,
        &format!("device02-operator-{suffix}@example.invalid"),
        false,
    )
    .await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'operator','active')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&operator).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&operator).bind(vec![9u8;32]).bind(vec![10u8;12]).execute(&pool).await.unwrap();
    let (operator_cookie, operator_csrf) = insert_org01_session(&pool, &operator, true).await;
    let mut operator_headers = headers.clone();
    operator_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={operator_cookie}")
            .parse()
            .unwrap(),
    );
    operator_headers.insert("x-csrf-token", operator_csrf.parse().unwrap());
    assert_eq!(
        rotate(state.clone(), operator_headers.clone(), device_id.clone())
            .await
            .err(),
        Some(StatusCode::FORBIDDEN),
        "operator without credentials:write grant is denied"
    );
    let membership_id = sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&operator)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO object_grants(id,tenant_id,membership_id,resource_type,device_id,permission) VALUES($1::uuid,$2::uuid,$3::uuid,'device',$4::uuid,'read'),($5::uuid,$2::uuid,$3::uuid,'device',$4::uuid,'credentials:write')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&membership_id).bind(&device_id).bind(uuid::Uuid::new_v4().to_string()).execute(&pool).await.unwrap();
    let operator_rotation = rotate(state.clone(), operator_headers.clone(), device_id.clone())
        .await
        .unwrap();
    assert_eq!(
        operator_rotation.status(),
        StatusCode::OK,
        "MFA-enrolled operator with object grant can rotate"
    );
    let operator_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(operator_rotation.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let operator_token = operator_json["token"].as_str().unwrap().to_owned();
    assert_eq!(
        update_device(
            State(state.clone()),
            operator_headers.clone(),
            axum::extract::Path(device_id.clone()),
            Json(serde_json::json!({"ingestion_policy":"always"})),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "operator cannot enable always-ingestion policy"
    );

    let viewer = insert_org01_user(
        &pool,
        &format!("device02-viewer-{suffix}@example.invalid"),
        false,
    )
    .await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'viewer','active')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&viewer).execute(&pool).await.unwrap();
    let (viewer_cookie, viewer_csrf) = insert_org01_session(&pool, &viewer, true).await;
    let mut viewer_headers = headers.clone();
    viewer_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={viewer_cookie}")
            .parse()
            .unwrap(),
    );
    viewer_headers.insert("x-csrf-token", viewer_csrf.parse().unwrap());
    assert_eq!(
        rotate(state.clone(), viewer_headers.clone(), device_id.clone())
            .await
            .err(),
        Some(StatusCode::FORBIDDEN),
        "viewer cannot issue device credentials"
    );
    assert_eq!(
        list_device_credentials(
            State(state.clone()),
            viewer_headers,
            axum::extract::Path(device_id.clone()),
            Query(PageQuery::default())
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "viewer cannot read credential metadata"
    );

    let mut ingest_before_disable =
        tracking_storage::begin_tenant_transaction(&tracking_pool, &tenant)
            .await
            .unwrap();
    tracking_storage::lock_tracking_resources(
        &mut ingest_before_disable,
        &tenant,
        std::slice::from_ref(&device_id),
        &[],
        &[],
    )
    .await
    .unwrap();
    let mut disable_task = tokio::spawn({
        let state = state.clone();
        let headers = headers.clone();
        let device_id = device_id.clone();
        async move {
            update_device(
                State(state),
                headers,
                axum::extract::Path(device_id),
                Json(serde_json::json!({"enabled":false})),
            )
            .await
        }
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), &mut disable_task)
            .await
            .is_err(),
        "disable waits for the shared device lock used by ingestion"
    );
    ingest_before_disable.commit().await.unwrap();
    let disabled = disable_task.await.unwrap().unwrap();
    assert!(!disabled.0.enabled);
    assert!(
        tracking_storage::lookup_device_credential(&tracking_pool, &digest(&operator_token))
            .await
            .unwrap()
            .is_none(),
        "disable revokes any credential and lookup rejects it"
    );
    assert_eq!(
        rotate(state.clone(), headers.clone(), device_id.clone())
            .await
            .err(),
        Some(StatusCode::CONFLICT),
        "cannot rotate while device is disabled"
    );
    let reenabled = update_device(
        State(state.clone()),
        headers.clone(),
        axum::extract::Path(device_id.clone()),
        Json(serde_json::json!({"enabled":true})),
    )
    .await
    .unwrap();
    assert!(reenabled.0.enabled);
    assert!(
        tracking_storage::lookup_device_credential(&tracking_pool, &digest(&operator_token))
            .await
            .unwrap()
            .is_none(),
        "re-enable never resurrects an old credential"
    );

    auth_pool.close().await;
    tracking_pool.close().await;
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn device03_application_token_lifecycle_and_audit_authorization() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let auth_role = format!("device03_auth_{}", &suffix[..8]);
    let app_role = format!("device03_app_{}", &suffix[..8]);
    for (role, password, capability) in [
        (&auth_role, "device03-auth-disposable", "gps_tracking_auth"),
        (&app_role, "device03-app-disposable", "gps_tracking_app"),
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
                .password("device03-auth-disposable"),
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
                .password("device03-app-disposable"),
        )
        .await
        .unwrap();
    let mut state =
        AppState::with_auth_test_key(auth_pool.clone(), "https://portal.example".into(), [93; 32]);
    state.set_tracking_pool(tracking_pool.clone());
    let tenant = uuid::Uuid::new_v4().to_string();
    let other_tenant = uuid::Uuid::new_v4().to_string();
    let admin = insert_org01_user(
        &pool,
        &format!("device03-admin-{suffix}@example.invalid"),
        false,
    )
    .await;
    let viewer = insert_org01_user(
        &pool,
        &format!("device03-viewer-{suffix}@example.invalid"),
        false,
    )
    .await;
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'DEVICE-03 fixture'),($2::uuid,'DEVICE-03 other tenant')")
        .bind(&tenant).bind(&other_tenant).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active'),($4::uuid,$2::uuid,$5::uuid,'viewer','active')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&admin)
        .bind(uuid::Uuid::new_v4().to_string()).bind(&viewer).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&admin).bind(vec![7u8;32]).bind(vec![8u8;12]).execute(&pool).await.unwrap();
    let (cookie, csrf) = insert_org01_session(&pool, &admin, true).await;
    let (stale_cookie, stale_csrf) = insert_org01_session(&pool, &admin, false).await;
    let (viewer_cookie, viewer_csrf) = insert_org01_session(&pool, &viewer, false).await;
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert("x-organization-id", tenant.parse().unwrap());
    headers.insert("x-csrf-token", csrf.parse().unwrap());
    headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    let mut stale_headers = headers.clone();
    stale_headers.insert("x-csrf-token", stale_csrf.parse().unwrap());
    stale_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={stale_cookie}")
            .parse()
            .unwrap(),
    );
    let device_id = uuid::Uuid::new_v4().to_string();
    let target_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'DEVICE-03 device','http-json',$3)")
        .bind(&device_id).bind(&tenant).bind(format!("d03-{suffix}")).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'DEVICE-03 target')",
    )
    .bind(&target_id)
    .bind(&tenant)
    .execute(&pool)
    .await
    .unwrap();

    let token_request = || ApplicationTokenRequest {
        name: "Map integration".into(),
        scopes: vec!["devices:read".into(), "positions:read".into()],
        device_ids: vec![device_id.clone()],
        target_ids: vec![target_id.clone()],
    };
    assert_eq!(
        issue_application_token(State(state.clone()), stale_headers, Json(token_request()))
            .await
            .err(),
        Some(StatusCode::FORBIDDEN),
        "issuance requires recent MFA-backed reauthentication"
    );
    assert_eq!(
        issue_application_token(
            State(state.clone()),
            headers.clone(),
            Json(ApplicationTokenRequest {
                name: "invalid empty allowlist".into(),
                scopes: vec!["devices:read".into()],
                device_ids: vec![],
                target_ids: vec![],
            }),
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );
    assert_eq!(
        issue_application_token(
            State(state.clone()),
            headers.clone(),
            Json(ApplicationTokenRequest {
                name: "forbidden management scope".into(),
                scopes: vec!["users:write".into()],
                device_ids: vec![device_id.clone()],
                target_ids: vec![],
            }),
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY)
    );

    let issued =
        issue_application_token(State(state.clone()), headers.clone(), Json(token_request()))
            .await
            .unwrap();
    assert_eq!(issued.status(), StatusCode::CREATED);
    assert_eq!(
        issued.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let request_id = issued
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(!request_id.is_empty());
    let issued_body = axum::body::to_bytes(issued.into_body(), usize::MAX)
        .await
        .unwrap();
    let issued_json: serde_json::Value = serde_json::from_slice(&issued_body).unwrap();
    let token_id = issued_json["id"].as_str().unwrap().to_owned();
    let secret = issued_json["token"].as_str().unwrap().to_owned();
    assert!(secret.starts_with("gps_app_"));
    assert_eq!(issued_json["device_ids"][0], device_id);
    assert_eq!(issued_json["target_ids"][0], target_id);
    let stored_digest: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM application_tokens WHERE id=$1::uuid")
            .bind(&token_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored_digest,
        digest(&secret),
        "only the token digest is stored"
    );
    assert_ne!(stored_digest, secret.as_bytes());
    let app_role_can_read_digest: bool = sqlx::query_scalar(
        "SELECT has_column_privilege(current_user,'application_tokens','token_hash','SELECT')",
    )
    .fetch_one(&tracking_pool)
    .await
    .unwrap();
    assert!(
        !app_role_can_read_digest,
        "API role cannot select raw token hashes"
    );
    let identity = tracking_storage::lookup_application_token(&tracking_pool, &digest(&secret))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(identity.token_id, token_id);
    assert_eq!(identity.tenant_id, tenant);
    assert_eq!(identity.owner_user_id, admin);
    assert!(identity.scopes.contains(&"devices:read".to_owned()));
    assert!(
        tracking_storage::lookup_application_token(&tracking_pool, &[0u8; 32])
            .await
            .unwrap()
            .is_none()
    );
    let expiry_is_90_days: bool = sqlx::query_scalar(
        "SELECT expires_at <= created_at + interval '90 days' AND expires_at > created_at FROM application_tokens WHERE id=$1::uuid",
    )
    .bind(&token_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(expiry_is_90_days);
    assert!(
        require_application_token_owner_permission(
            &auth_pool, &token_id, &tenant, "device", &device_id, "read",
        )
        .await
        .is_ok(),
        "token scope, explicit device allowlist and current owner permission intersect"
    );

    let second_response = issue_application_token(
        State(state.clone()),
        headers.clone(),
        Json(ApplicationTokenRequest {
            name: "Target integration".into(),
            scopes: vec!["targets:read".into()],
            device_ids: vec![],
            target_ids: vec![target_id.clone()],
        }),
    )
    .await
    .unwrap();
    let second_body = axum::body::to_bytes(second_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let second_json: serde_json::Value = serde_json::from_slice(&second_body).unwrap();
    let second_token = second_json["token"].as_str().unwrap().to_owned();
    let token_page = list_application_tokens(
        State(state.clone()),
        headers.clone(),
        Query(PageQuery {
            limit: Some(1),
            cursor: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        token_page.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let token_page_body = axum::body::to_bytes(token_page.into_body(), usize::MAX)
        .await
        .unwrap();
    let token_page_json: serde_json::Value = serde_json::from_slice(&token_page_body).unwrap();
    assert_eq!(token_page_json["data"].as_array().unwrap().len(), 1);
    assert!(token_page_json["next_cursor"].as_str().is_some());
    let page_text = token_page_json.to_string();
    assert!(!page_text.contains(&secret) && !page_text.contains(&second_token));
    assert!(!page_text.contains("token_hash"));
    let cursor = token_page_json["next_cursor"].as_str().unwrap().to_owned();
    let second_page = list_application_tokens(
        State(state.clone()),
        headers.clone(),
        Query(PageQuery {
            limit: Some(1),
            cursor: Some(cursor.clone()),
        }),
    )
    .await
    .unwrap();
    let second_page_body = axum::body::to_bytes(second_page.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&second_page_body).unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut tampered_cursor = cursor;
    tampered_cursor.push('x');
    assert_eq!(
        list_application_tokens(
            State(state.clone()),
            headers.clone(),
            Query(PageQuery {
                limit: Some(1),
                cursor: Some(tampered_cursor),
            }),
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST)
    );

    let mut viewer_headers = headers.clone();
    viewer_headers.insert("x-csrf-token", viewer_csrf.parse().unwrap());
    viewer_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={viewer_cookie}")
            .parse()
            .unwrap(),
    );
    assert_eq!(
        list_application_tokens(
            State(state.clone()),
            viewer_headers.clone(),
            Query(PageQuery::default()),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "viewer cannot list application token metadata"
    );
    assert_eq!(
        revoke_application_token(
            State(state.clone()),
            viewer_headers,
            axum::extract::Path(token_id.clone()),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "viewer cannot revoke an application token"
    );

    sqlx::query(
        "UPDATE memberships SET status='suspended' WHERE tenant_id=$1::uuid AND user_id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&admin)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        require_application_token_owner_permission(
            &auth_pool, &token_id, &tenant, "device", &device_id, "read",
        )
        .await,
        Err(StatusCode::FORBIDDEN),
        "existing token no longer works after owner membership is suspended"
    );
    assert!(
        tracking_storage::lookup_application_token(&tracking_pool, &digest(&secret))
            .await
            .unwrap()
            .is_none(),
        "lookup rejects an owner whose membership has been suspended"
    );
    sqlx::query(
        "UPDATE memberships SET status='active' WHERE tenant_id=$1::uuid AND user_id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&admin)
    .execute(&pool)
    .await
    .unwrap();

    let other_resource_id = uuid::Uuid::new_v4().to_string();
    for (event_tenant, resource, request) in [
        (tenant.as_str(), device_id.as_str(), "fixture-audit-1"),
        (tenant.as_str(), device_id.as_str(), "fixture-audit-2"),
        (
            other_tenant.as_str(),
            other_resource_id.as_str(),
            "fixture-audit-other",
        ),
    ] {
        sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES($1::uuid,$2,'fixture.audit','device',$3::uuid,'success',$4)")
            .bind(event_tenant).bind(format!("user:{admin}")).bind(resource).bind(request).execute(&pool).await.unwrap();
    }
    let audit_query = || AuditQuery {
        from_time: None,
        to_time: None,
        actor_id: Some(admin.clone()),
        action: Some("fixture.audit".into()),
        resource_id: Some(device_id.clone()),
        limit: Some(1),
        cursor: None,
    };
    let audit_page = list_audit_events(State(state.clone()), headers.clone(), Query(audit_query()))
        .await
        .unwrap();
    let audit_body = axum::body::to_bytes(audit_page.into_body(), usize::MAX)
        .await
        .unwrap();
    let audit_json: serde_json::Value = serde_json::from_slice(&audit_body).unwrap();
    assert_eq!(audit_json["data"].as_array().unwrap().len(), 1);
    let event = &audit_json["data"][0];
    let audit_id = event["id"].as_str().unwrap();
    assert!(
        audit_id.parse::<i64>().unwrap() > 0,
        "BIGINT ID is a decimal string"
    );
    assert_eq!(event["tenant_id"], tenant);
    assert_eq!(event["actor_id"], admin);
    assert_eq!(event["resource_id"], device_id);
    assert!(audit_json["next_cursor"].as_str().is_some());
    let mut next_audit_query = audit_query();
    next_audit_query.cursor = audit_json["next_cursor"].as_str().map(str::to_owned);
    let next_audit_page = list_audit_events(
        State(state.clone()),
        headers.clone(),
        Query(next_audit_query),
    )
    .await
    .unwrap();
    let next_audit_body = axum::body::to_bytes(next_audit_page.into_body(), usize::MAX)
        .await
        .unwrap();
    let next_audit_json: serde_json::Value = serde_json::from_slice(&next_audit_body).unwrap();
    assert_eq!(next_audit_json["data"].as_array().unwrap().len(), 1);
    assert_ne!(next_audit_json["data"][0]["id"], audit_id);
    assert!(next_audit_json["next_cursor"].is_null());
    assert_eq!(
        list_audit_events(
            State(state.clone()),
            headers.clone(),
            Query(AuditQuery {
                from_time: None,
                to_time: None,
                actor_id: None,
                action: Some("other.filter".into()),
                resource_id: Some(device_id.clone()),
                limit: Some(1),
                cursor: audit_json["next_cursor"].as_str().map(str::to_owned),
            }),
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST),
        "audit cursor is bound to tenant and filters"
    );
    assert_eq!(
        list_audit_events(
            State(state.clone()),
            headers.clone(),
            Query(AuditQuery {
                from_time: Some("not-a-date".into()),
                to_time: None,
                ..AuditQuery::default()
            }),
        )
        .await
        .err(),
        Some(StatusCode::BAD_REQUEST),
        "malformed RFC3339 range is rejected"
    );
    assert_eq!(
        list_audit_events(
            State(state.clone()),
            headers.clone(),
            Query(AuditQuery {
                from_time: Some("2026-01-01T00:00:00Z".into()),
                to_time: Some("2026-02-02T00:00:00Z".into()),
                ..AuditQuery::default()
            }),
        )
        .await
        .err(),
        Some(StatusCode::UNPROCESSABLE_ENTITY),
        "audit query range cannot exceed 31 days"
    );

    let token_revocation = revoke_application_token(
        State(state.clone()),
        headers.clone(),
        axum::extract::Path(token_id.clone()),
    )
    .await
    .unwrap();
    assert_eq!(token_revocation.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        token_revocation
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap(),
        "no-store"
    );
    assert_eq!(
        require_application_token_owner_permission(
            &auth_pool, &token_id, &tenant, "device", &device_id, "read",
        )
        .await,
        Err(StatusCode::FORBIDDEN),
        "revoked token is no longer authorized"
    );
    assert!(
        tracking_storage::lookup_application_token(&tracking_pool, &digest(&secret))
            .await
            .unwrap()
            .is_none(),
        "lookup rejects a revoked token"
    );
    let revoked_again = revoke_application_token(
        State(state.clone()),
        headers.clone(),
        axum::extract::Path(token_id.clone()),
    )
    .await
    .unwrap();
    assert_eq!(revoked_again.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        revoke_application_token(
            State(state.clone()),
            headers.clone(),
            axum::extract::Path(uuid::Uuid::new_v4().to_string()),
        )
        .await
        .err(),
        Some(StatusCode::NOT_FOUND)
    );
    let after_revoke = list_application_tokens(
        State(state.clone()),
        headers.clone(),
        Query(PageQuery::default()),
    )
    .await
    .unwrap();
    let after_revoke_body = axum::body::to_bytes(after_revoke.into_body(), usize::MAX)
        .await
        .unwrap();
    let after_revoke_json: serde_json::Value = serde_json::from_slice(&after_revoke_body).unwrap();
    assert_eq!(after_revoke_json["data"].as_array().unwrap().len(), 1);

    auth_pool.close().await;
    tracking_pool.close().await;
    pool.close().await;
}
