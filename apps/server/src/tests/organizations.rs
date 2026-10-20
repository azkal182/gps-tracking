use super::*;

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
async fn org01_platform_tenant_and_invitation_lifecycle() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let auth_role = format!("org01_auth_{}", &suffix[..8]);
    let platform_role = format!("org01_platform_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {auth_role} LOGIN PASSWORD 'org01-auth-disposable-password'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT gps_tracking_auth TO {auth_role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE ROLE {platform_role} LOGIN PASSWORD 'org01-platform-disposable-password'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "GRANT gps_tracking_platform_provisioner TO {platform_role}"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let auth_pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&auth_role)
                .password("org01-auth-disposable-password"),
        )
        .await
        .unwrap();
    let platform_pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SET ROLE gps_tracking_platform_provisioner")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&platform_role)
                .password("org01-platform-disposable-password"),
        )
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT current_user")
            .fetch_one(&platform_pool)
            .await
            .unwrap(),
        "gps_tracking_platform_provisioner"
    );
    let mut platform_probe = platform_pool.begin().await.unwrap();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'ORG-01 capability probe')")
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&mut *platform_probe)
        .await
        .unwrap();
    platform_probe.rollback().await.unwrap();
    let mut state =
        AppState::with_auth_test_key(auth_pool.clone(), "https://portal.example".into(), [61; 32]);
    if let Some(auth) = state.auth.as_mut() {
        auth.outbox_key = Some([62; 32]);
        auth.platform_pool = Some(platform_pool.clone());
    }

    let platform_id =
        insert_org01_user(&pool, &format!("platform-{suffix}@example.invalid"), true).await;
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&platform_id).bind(vec![7u8;32]).bind(vec![8u8;12]).execute(&pool).await.unwrap();
    let (platform_cookie, platform_csrf) = insert_org01_session(&pool, &platform_id, true).await;
    let mut platform_headers = HeaderMap::new();
    platform_headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    platform_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={platform_cookie}")
            .parse()
            .unwrap(),
    );
    platform_headers.insert("x-csrf-token", platform_csrf.parse().unwrap());

    let created = create_organization(
        State(state.clone()),
        platform_headers.clone(),
        Json(OrganizationCreateRequest {
            name: "ORG-01 Synthetic Org".into(),
            admin_email: "Initial.Admin@example.invalid".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(created.0, StatusCode::CREATED);
    let organization_id = created.1.0.organization.id.clone();
    assert_eq!(created.1.0.organization.status, "active");
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM email_outbox WHERE tenant_id=$1::uuid AND template='invitation'",
    )
    .bind(&organization_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        outbox_count, 1,
        "initial admin invite and encrypted email are atomic"
    );
    assert!(
        require_platform_admin(
            &state.auth.clone().unwrap(),
            &authenticate_cookie_session(&auth_pool, &platform_headers, false)
                .await
                .unwrap()
        )
        .await
        .is_ok()
    );
    assert_eq!(
        reissue_initial_admin_invitation(
            State(state.clone()),
            axum::extract::Path(organization_id.clone()),
            platform_headers.clone(),
            Json(InvitationReissueRequest {
                email: "replacement.admin@example.invalid".into()
            })
        )
        .await
        .unwrap()
        .0,
        StatusCode::CREATED
    );

    let tenant_admin = insert_org01_user(
        &pool,
        &format!("tenant.admin-{suffix}@example.invalid"),
        false,
    )
    .await;
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')").bind(uuid::Uuid::new_v4().to_string()).bind(&organization_id).bind(&tenant_admin).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())").bind(uuid::Uuid::new_v4().to_string()).bind(&tenant_admin).bind(vec![9u8;32]).bind(vec![10u8;12]).execute(&pool).await.unwrap();
    let (admin_cookie, admin_csrf) = insert_org01_session(&pool, &tenant_admin, true).await;
    let mut tenant_headers = HeaderMap::new();
    tenant_headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    tenant_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={admin_cookie}")
            .parse()
            .unwrap(),
    );
    tenant_headers.insert("x-csrf-token", admin_csrf.parse().unwrap());
    tenant_headers.insert("x-organization-id", organization_id.parse().unwrap());
    assert!(
        get_organization(State(state.clone()), tenant_headers.clone())
            .await
            .is_ok()
    );

    let invitation = create_invitation(
        State(state.clone()),
        tenant_headers.clone(),
        Json(InvitationCreateRequest {
            email: "new.admin@example.invalid".into(),
            role: "admin".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(invitation.0, StatusCode::CREATED);
    assert_eq!(invitation.1.0.role, "admin");
    assert_eq!(
        create_invitation(
            State(state.clone()),
            tenant_headers.clone(),
            Json(InvitationCreateRequest {
                email: "NEW.ADMIN@example.invalid".into(),
                role: "admin".into()
            })
        )
        .await
        .err(),
        Some(StatusCode::CONFLICT)
    );
    let encrypted:Vec<u8>=sqlx::query_scalar("SELECT encrypted_payload FROM email_outbox WHERE recipient='new.admin@example.invalid' ORDER BY created_at DESC LIMIT 1").fetch_one(&pool).await.unwrap();
    let plain = mfa::decrypt(&[62; 32], &encrypted[12..], &encrypted[..12]).unwrap();
    let payload: email::Payload = serde_json::from_slice(&plain).unwrap();
    assert!(
        !serde_json::to_string(&invitation.1.0)
            .unwrap()
            .contains(&payload.token)
    );
    let old_hash: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM invitations WHERE id=$1::uuid")
            .bind(&invitation.1.0.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        resend_invitation(
            State(state.clone()),
            axum::extract::Path(invitation.1.0.id.clone()),
            tenant_headers.clone(),
            Json(EmptyRequest {})
        )
        .await
        .unwrap(),
        StatusCode::ACCEPTED
    );
    let new_hash: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM invitations WHERE id=$1::uuid")
            .bind(&invitation.1.0.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(old_hash, new_hash, "resend invalidates the prior token");
    assert_eq!(
        revoke_invitation(
            State(state.clone()),
            axum::extract::Path(invitation.1.0.id.clone()),
            tenant_headers.clone()
        )
        .await
        .unwrap(),
        StatusCode::NO_CONTENT
    );

    let other_org_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'Other ORG-01 tenant')")
        .bind(&other_org_id)
        .execute(&pool)
        .await
        .unwrap();
    let mut wrong_tenant = tenant_headers.clone();
    wrong_tenant.insert("x-organization-id", other_org_id.parse().unwrap());
    assert_eq!(
        create_invitation(
            State(state.clone()),
            wrong_tenant,
            Json(InvitationCreateRequest {
                email: "cross@example.invalid".into(),
                role: "viewer".into()
            })
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN)
    );

    let accept_token = random_secret();
    let accept_id = uuid::Uuid::new_v4().to_string();
    let new_admin_email = format!("new.accept-{suffix}@example.invalid");
    sqlx::query("INSERT INTO invitations(id,tenant_id,email_canonical,role,token_hash,expires_at) VALUES($1::uuid,$2::uuid,$3,'organization_admin',$4,clock_timestamp()+interval '72 hours')").bind(&accept_id).bind(&organization_id).bind(&new_admin_email).bind(digest(&accept_token)).execute(&pool).await.unwrap();
    let mut preauth = HeaderMap::new();
    preauth.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    preauth.insert(
        header::COOKIE,
        "__Host-tracking_pre_auth=preauth-fixture".parse().unwrap(),
    );
    preauth.insert("x-csrf-token", "preauth-fixture".parse().unwrap());
    let accepted_new = accept_invitation(
        State(state.clone()),
        preauth.clone(),
        Json(InvitationAcceptRequest {
            token: accept_token.clone(),
            password: Some("a synthetic invite password".into()),
            display_name: Some("New Example".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(accepted_new.0["state"], "login_required");
    assert_eq!(
        accept_invitation(
            State(state.clone()),
            preauth.clone(),
            Json(InvitationAcceptRequest {
                token: accept_token.clone(),
                password: None,
                display_name: None
            })
        )
        .await
        .err(),
        Some(StatusCode::UNAUTHORIZED),
        "accept token is one use"
    );
    let (accept_user,membership_status):(String,String)=sqlx::query_as("SELECT u.id::text,m.status FROM users u JOIN memberships m ON m.user_id=u.id WHERE u.email_canonical=$2 AND m.tenant_id=$1::uuid").bind(&organization_id).bind(&new_admin_email).fetch_one(&pool).await.unwrap();
    assert_eq!(
        membership_status, "pending_mfa",
        "new organization admin must enroll MFA before privileged access"
    );

    let matching_email = format!("match-{suffix}@example.invalid");
    let existing_id = insert_org01_user(&pool, &matching_email, false).await;
    let matching_token = random_secret();
    sqlx::query("INSERT INTO invitations(id,tenant_id,email_canonical,role,token_hash,expires_at) VALUES($1::uuid,$2::uuid,$3,'viewer',$4,clock_timestamp()+interval '72 hours')").bind(uuid::Uuid::new_v4().to_string()).bind(&organization_id).bind(&matching_email).bind(digest(&matching_token)).execute(&pool).await.unwrap();
    let (matching_cookie, matching_csrf) = insert_org01_session(&pool, &existing_id, false).await;
    let mut matching_headers = HeaderMap::new();
    matching_headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    matching_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={matching_cookie}")
            .parse()
            .unwrap(),
    );
    matching_headers.insert("x-csrf-token", matching_csrf.parse().unwrap());
    let matching = accept_invitation(
        State(state.clone()),
        matching_headers,
        Json(InvitationAcceptRequest {
            token: matching_token,
            password: None,
            display_name: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(matching.0["state"], "accepted");
    assert_eq!(matching.0["membership"]["role"], "viewer");

    let mismatch_token = random_secret();
    sqlx::query("INSERT INTO invitations(id,tenant_id,email_canonical,role,token_hash,expires_at) VALUES($1::uuid,$2::uuid,$3,'viewer',$4,clock_timestamp()+interval '72 hours')").bind(uuid::Uuid::new_v4().to_string()).bind(&organization_id).bind(format!("wrong.person-{suffix}@example.invalid")).bind(digest(&mismatch_token)).execute(&pool).await.unwrap();
    let (accept_cookie, accept_csrf) = insert_org01_session(&pool, &accept_user, false).await;
    let mut signed_headers = HeaderMap::new();
    signed_headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    signed_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={accept_cookie}")
            .parse()
            .unwrap(),
    );
    signed_headers.insert("x-csrf-token", accept_csrf.parse().unwrap());
    assert_eq!(
        accept_invitation(
            State(state.clone()),
            signed_headers.clone(),
            Json(InvitationAcceptRequest {
                token: mismatch_token,
                password: None,
                display_name: None
            })
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN)
    );

    let suspended_org = set_organization_status(
        State(state.clone()),
        axum::extract::Path(organization_id.clone()),
        platform_headers.clone(),
        Json(OrganizationStatusRequest {
            status: "suspended".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(suspended_org.0.status, "suspended");
    assert_eq!(
        get_organization(State(state.clone()), tenant_headers.clone())
            .await
            .err(),
        Some(StatusCode::UNAUTHORIZED)
    );
    let status: String = sqlx::query_scalar("SELECT status FROM organizations WHERE id=$1::uuid")
        .bind(&organization_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "suspended");
    let reactivated_org = set_organization_status(
        State(state.clone()),
        axum::extract::Path(organization_id.clone()),
        platform_headers.clone(),
        Json(OrganizationStatusRequest {
            status: "active".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(reactivated_org.0.status, "active");
    let platform_list = list_platform_organizations(
        State(state.clone()),
        platform_headers.clone(),
        Query(PageQuery {
            limit: Some(1),
            cursor: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(platform_list.0.data.len(), 1);
    let next = platform_list
        .0
        .next_cursor
        .clone()
        .expect("second page exists");
    let second_page = list_platform_organizations(
        State(state.clone()),
        platform_headers.clone(),
        Query(PageQuery {
            limit: Some(1),
            cursor: Some(next.clone()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(second_page.0.data.len(), 1);
    assert_ne!(platform_list.0.data[0].id, second_page.0.data[0].id);
    let mut tampered = next.into_bytes();
    if let Some(byte) = tampered.last_mut() {
        *byte = if *byte == b'A' { b'B' } else { b'A' };
    }
    let tampered = String::from_utf8(tampered).unwrap();
    assert!(matches!(
        list_platform_organizations(
            State(state.clone()),
            platform_headers.clone(),
            Query(PageQuery {
                limit: Some(1),
                cursor: Some(tampered)
            }),
        )
        .await,
        Err(StatusCode::BAD_REQUEST)
    ));
    assert!(
        platform_list
            .0
            .data
            .iter()
            .all(|o| !serde_json::to_string(o).unwrap().contains("coordinates"))
    );

    let active_admin: bool =
        sqlx::query_scalar("SELECT gps_tracking_organization_has_active_admin($1::uuid)")
            .bind(&organization_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(active_admin);
    sqlx::query("DELETE FROM audit_events WHERE tenant_id IN ($1::uuid,$2::uuid)")
        .bind(&organization_id)
        .bind(&other_org_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM email_outbox WHERE tenant_id=$1::uuid")
        .bind(&organization_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM email_outbox WHERE tenant_id=$1::uuid")
        .bind(&other_org_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM memberships WHERE tenant_id IN ($1::uuid,$2::uuid)")
        .bind(&organization_id)
        .bind(&other_org_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM invitations WHERE tenant_id IN ($1::uuid,$2::uuid)")
        .bind(&organization_id)
        .bind(&other_org_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organizations WHERE id IN ($1::uuid,$2::uuid)")
        .bind(&organization_id)
        .bind(&other_org_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM auth_sessions WHERE user_id IN ($1::uuid,$2::uuid,$3::uuid,$4::uuid)")
        .bind(&platform_id)
        .bind(&tenant_admin)
        .bind(&accept_user)
        .bind(&existing_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id IN ($1::uuid,$2::uuid,$3::uuid,$4::uuid)")
        .bind(&platform_id)
        .bind(&tenant_admin)
        .bind(&accept_user)
        .bind(&existing_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL/PostGIS database"]
async fn org02_membership_grants_last_admin_and_operator_mfa() {
    let database_url = std::env::var("GPS_TEST_DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let auth_role = format!("org02_auth_{}", &suffix[..8]);
    sqlx::query(&format!(
        "CREATE ROLE {auth_role} LOGIN PASSWORD 'org02-auth-disposable-password'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT gps_tracking_auth TO {auth_role}"))
        .execute(&pool)
        .await
        .unwrap();
    let auth_pool = sqlx::postgres::PgPoolOptions::new()
        .connect_with(
            sqlx::postgres::PgConnectOptions::from_str(&database_url)
                .unwrap()
                .username(&auth_role)
                .password("org02-auth-disposable-password"),
        )
        .await
        .unwrap();
    let state =
        AppState::with_auth_test_key(auth_pool.clone(), "https://portal.example".into(), [71; 32]);

    let tenant = uuid::Uuid::new_v4().to_string();
    let other_tenant = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'ORG-02 fixture'),($2::uuid,'ORG-02 other tenant')")
            .bind(&tenant).bind(&other_tenant).execute(&pool).await.unwrap();
    let admin = insert_org01_user(
        &pool,
        &format!("org02-admin-{suffix}@example.invalid"),
        false,
    )
    .await;
    let second_admin = insert_org01_user(
        &pool,
        &format!("org02-admin2-{suffix}@example.invalid"),
        false,
    )
    .await;
    let operator = insert_org01_user(
        &pool,
        &format!("org02-operator-{suffix}@example.invalid"),
        false,
    )
    .await;
    let viewer = insert_org01_user(
        &pool,
        &format!("org02-viewer-{suffix}@example.invalid"),
        false,
    )
    .await;
    let promoted = insert_org01_user(
        &pool,
        &format!("org02-promote-{suffix}@example.invalid"),
        false,
    )
    .await;
    let user_ids = [&admin, &second_admin, &operator, &viewer, &promoted];
    for (index, user_id) in user_ids.iter().enumerate() {
        let role = if index < 2 {
            "organization_admin"
        } else if index == 2 {
            "operator"
        } else {
            "viewer"
        };
        sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,$4,'active')")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(user_id).bind(role)
                .execute(&pool).await.unwrap();
    }
    for user_id in [&admin, &second_admin] {
        sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())")
                .bind(uuid::Uuid::new_v4().to_string()).bind(user_id).bind(vec![71u8;32]).bind(vec![72u8;12])
                .execute(&pool).await.unwrap();
    }
    let (cookie, csrf) = insert_org01_session(&pool, &admin, true).await;
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={cookie}").parse().unwrap(),
    );
    headers.insert("x-csrf-token", csrf.parse().unwrap());
    headers.insert("x-organization-id", tenant.parse().unwrap());

    let member_page = list_members(
        State(state.clone()),
        headers.clone(),
        Query(PageQuery {
            limit: Some(1),
            cursor: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(member_page.0.data.len(), 1);
    assert!(
        member_page.0.next_cursor.is_some(),
        "membership list signs the next page cursor"
    );
    assert!(
        !serde_json::to_string(&member_page.0)
            .unwrap()
            .contains("password_hash")
    );

    let target = uuid::Uuid::new_v4().to_string();
    let device = uuid::Uuid::new_v4().to_string();
    let creator_device = uuid::Uuid::new_v4().to_string();
    let foreign_device = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO targets(id,tenant_id,name) VALUES($1::uuid,$2::uuid,'ORG-02 target')")
        .bind(&target)
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'ORG-02 device','synthetic','org02-device')")
            .bind(&device).bind(&tenant).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'Creator device','synthetic','org02-creator-device')")
            .bind(&creator_device).bind(&tenant).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier) VALUES($1::uuid,$2::uuid,'Foreign device','synthetic','org02-foreign-device')")
            .bind(&foreign_device).bind(&other_tenant).execute(&pool).await.unwrap();
    let mut creator_tx = tracking_storage::begin_tenant_transaction(&auth_pool, &tenant)
        .await
        .unwrap();
    grant_creator_object_access(
        &mut creator_tx,
        &tenant,
        &operator,
        "device",
        &creator_device,
    )
    .await
    .unwrap();
    creator_tx.commit().await.unwrap();
    require_object_permission(
        &auth_pool,
        &tenant,
        &operator,
        "device",
        &creator_device,
        "read",
    )
    .await
    .unwrap();
    require_object_permission(
        &auth_pool,
        &tenant,
        &operator,
        "device",
        &creator_device,
        "write",
    )
    .await
    .unwrap();

    let initial_policy: bool =
        sqlx::query_scalar("SELECT gps_tracking_user_requires_mfa($1::uuid)")
            .bind(&operator)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!initial_policy);
    let grant_request = GrantReplaceRequest {
        grants: vec![
            GrantInput {
                resource_type: "device".into(),
                resource_id: device.clone(),
                permissions: vec!["credentials:write".into()],
            },
            GrantInput {
                resource_type: "target".into(),
                resource_id: target.clone(),
                permissions: vec!["read".into()],
            },
            GrantInput {
                resource_type: "organization".into(),
                resource_id: tenant.clone(),
                permissions: vec!["devices:create".into()],
            },
        ],
    };
    let grants = replace_member_grants(
        State(state.clone()),
        axum::extract::Path(operator.clone()),
        headers.clone(),
        Json(grant_request),
    )
    .await
    .unwrap();
    assert_eq!(grants.0.grants.len(), 3);
    let policy_after_grant: bool =
        sqlx::query_scalar("SELECT gps_tracking_user_requires_mfa($1::uuid)")
            .bind(&operator)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        policy_after_grant,
        "active operator credentials:write requires MFA"
    );
    require_object_permission(&auth_pool, &tenant, &operator, "device", &device, "read")
        .await
        .unwrap();
    require_object_permission(
        &auth_pool,
        &tenant,
        &operator,
        "device",
        &device,
        "credentials:write",
    )
    .await
    .unwrap();
    let app_token_id = uuid::Uuid::new_v4().to_string();
    let mut app_token_tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) VALUES($1::uuid,$2::uuid,$3::uuid,'ORG-02 synthetic owner token',$4,ARRAY['devices:read'],clock_timestamp()+interval '30 days')")
            .bind(&app_token_id).bind(&tenant).bind(&operator).bind(digest(&random_secret()))
            .execute(&mut *app_token_tx).await.unwrap();
    sqlx::query("INSERT INTO application_token_devices(tenant_id,application_token_id,device_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
            .bind(&tenant).bind(&app_token_id).bind(&device).execute(&mut *app_token_tx).await.unwrap();
    app_token_tx.commit().await.unwrap();
    require_application_token_owner_permission(
        &auth_pool,
        &app_token_id,
        &tenant,
        "device",
        &device,
        "read",
    )
    .await
    .unwrap();
    assert_eq!(
        require_object_permission(
            &auth_pool,
            &other_tenant,
            &operator,
            "device",
            &device,
            "read"
        )
        .await,
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        replace_member_grants(
            State(state.clone()),
            axum::extract::Path(operator.clone()),
            headers.clone(),
            Json(GrantReplaceRequest {
                grants: vec![GrantInput {
                    resource_type: "device".into(),
                    resource_id: foreign_device.clone(),
                    permissions: vec!["read".into()]
                }]
            })
        )
        .await
        .err(),
        Some(StatusCode::NOT_FOUND)
    );
    let listed = list_member_grants(
        State(state.clone()),
        axum::extract::Path(operator.clone()),
        headers.clone(),
    )
    .await
    .unwrap();
    assert_eq!(listed.0.grants.len(), 3);
    let (operator_cookie, operator_csrf) = insert_org01_session(&pool, &operator, false).await;
    let mut operator_headers = HeaderMap::new();
    operator_headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    operator_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={operator_cookie}")
            .parse()
            .unwrap(),
    );
    operator_headers.insert("x-csrf-token", operator_csrf.parse().unwrap());
    operator_headers.insert("x-organization-id", tenant.parse().unwrap());
    let own_grants = list_member_grants(
        State(state.clone()),
        axum::extract::Path(operator.clone()),
        operator_headers.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        own_grants.0.grants.len(),
        3,
        "members can read only their own grants"
    );
    assert_eq!(
        list_member_grants(
            State(state.clone()),
            axum::extract::Path(admin.clone()),
            operator_headers.clone(),
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "non-admin member cannot inspect another member's grants"
    );
    let mut other_tenant_headers = operator_headers.clone();
    other_tenant_headers.insert("x-organization-id", other_tenant.parse().unwrap());
    assert_eq!(
        list_member_grants(
            State(state.clone()),
            axum::extract::Path(operator.clone()),
            other_tenant_headers,
        )
        .await
        .err(),
        Some(StatusCode::FORBIDDEN),
        "member cannot read their grant list outside an active tenant membership"
    );

    let viewer_grants = replace_member_grants(
        State(state.clone()),
        axum::extract::Path(viewer.clone()),
        headers.clone(),
        Json(GrantReplaceRequest {
            grants: vec![GrantInput {
                resource_type: "target".into(),
                resource_id: target.clone(),
                permissions: vec!["read".into()],
            }],
        }),
    )
    .await
    .unwrap();
    assert_eq!(viewer_grants.0.grants.len(), 1);
    require_object_permission(&auth_pool, &tenant, &viewer, "target", &target, "read")
        .await
        .unwrap();
    assert_eq!(
        require_object_permission(&auth_pool, &tenant, &viewer, "target", &target, "write").await,
        Err(StatusCode::FORBIDDEN)
    );

    let empty = replace_member_grants(
        State(state.clone()),
        axum::extract::Path(operator.clone()),
        headers.clone(),
        Json(GrantReplaceRequest { grants: vec![] }),
    )
    .await
    .unwrap();
    assert!(empty.0.grants.is_empty());
    let policy_after_revoke: bool =
        sqlx::query_scalar("SELECT gps_tracking_user_requires_mfa($1::uuid)")
            .bind(&operator)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !policy_after_revoke,
        "replacing away credentials:write removes the MFA requirement"
    );
    assert_eq!(
        require_application_token_owner_permission(
            &auth_pool,
            &app_token_id,
            &tenant,
            "device",
            &device,
            "read"
        )
        .await,
        Err(StatusCode::FORBIDDEN),
        "integration owner grants are evaluated live"
    );
    assert_eq!(
        require_object_permission(&auth_pool, &tenant, &operator, "device", &device, "read").await,
        Err(StatusCode::FORBIDDEN)
    );
    let _restored_grants = replace_member_grants(
        State(state.clone()),
        axum::extract::Path(operator.clone()),
        headers.clone(),
        Json(GrantReplaceRequest {
            grants: vec![GrantInput {
                resource_type: "device".into(),
                resource_id: device.clone(),
                permissions: vec!["credentials:write".into()],
            }],
        }),
    )
    .await
    .unwrap();
    let downgraded = update_member(
        State(state.clone()),
        axum::extract::Path(operator.clone()),
        headers.clone(),
        Json(MembershipPatchRequest {
            role: Some("viewer".into()),
            status: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(downgraded.0.role, "viewer");
    let policy_after_downgrade: bool =
        sqlx::query_scalar("SELECT gps_tracking_user_requires_mfa($1::uuid)")
            .bind(&operator)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !policy_after_downgrade,
        "credentials grant on a non-operator does not impose operator MFA policy"
    );
    assert_eq!(
        require_object_permission(
            &auth_pool,
            &tenant,
            &operator,
            "device",
            &device,
            "credentials:write"
        )
        .await,
        Err(StatusCode::FORBIDDEN)
    );

    let restored_operator = update_member(
        State(state.clone()),
        axum::extract::Path(operator.clone()),
        headers.clone(),
        Json(MembershipPatchRequest {
            role: Some("operator".into()),
            status: Some("active".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(restored_operator.0.role, "operator");
    assert_eq!(
        remove_member(
            State(state.clone()),
            axum::extract::Path(operator.clone()),
            headers.clone()
        )
        .await
        .unwrap(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        require_application_token_owner_permission(
            &auth_pool,
            &app_token_id,
            &tenant,
            "device",
            &device,
            "read"
        )
        .await,
        Err(StatusCode::FORBIDDEN)
    );
    let token_revoked: bool = sqlx::query_scalar(
        "SELECT revoked_at IS NOT NULL FROM application_tokens WHERE id=$1::uuid",
    )
    .bind(&app_token_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        token_revoked,
        "removing an integration owner revokes the token"
    );

    let promoted_member = update_member(
        State(state.clone()),
        axum::extract::Path(promoted.clone()),
        headers.clone(),
        Json(MembershipPatchRequest {
            role: Some("admin".into()),
            status: Some("active".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        promoted_member.0.status, "pending_mfa",
        "admin promotion cannot bypass MFA enrollment"
    );
    let promote_policy: bool =
        sqlx::query_scalar("SELECT gps_tracking_user_requires_mfa($1::uuid)")
            .bind(&promoted)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(promote_policy);

    let (self_result, second_result) = tokio::join!(
        update_member(
            State(state.clone()),
            axum::extract::Path(admin.clone()),
            headers.clone(),
            Json(MembershipPatchRequest {
                role: Some("viewer".into()),
                status: None
            })
        ),
        update_member(
            State(state.clone()),
            axum::extract::Path(second_admin.clone()),
            headers.clone(),
            Json(MembershipPatchRequest {
                role: Some("viewer".into()),
                status: None
            })
        ),
    );
    let successes = usize::from(self_result.is_ok()) + usize::from(second_result.is_ok());
    assert_eq!(
        successes, 1,
        "concurrent changes cannot remove all active MFA-enrolled admins"
    );
    let active_admins: i64 = sqlx::query_scalar("SELECT count(*) FROM memberships m JOIN users u ON u.id=m.user_id JOIN mfa_factors f ON f.user_id=u.id AND f.enabled_at IS NOT NULL WHERE m.tenant_id=$1::uuid AND m.role='organization_admin' AND m.status='active' AND u.status='active'")
            .bind(&tenant).fetch_one(&pool).await.unwrap();
    assert_eq!(active_admins, 1);

    sqlx::query("DELETE FROM application_tokens WHERE tenant_id=$1::uuid")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM object_grants WHERE tenant_id IN ($1::uuid,$2::uuid)")
        .bind(&tenant)
        .bind(&other_tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM audit_events WHERE tenant_id IN ($1::uuid,$2::uuid)")
        .bind(&tenant)
        .bind(&other_tenant)
        .execute(&pool)
        .await
        .unwrap();
    for user_id in user_ids {
        sqlx::query("DELETE FROM auth_sessions WHERE user_id=$1::uuid")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM mfa_factors WHERE user_id=$1::uuid")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM memberships WHERE tenant_id=$1::uuid")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM targets WHERE tenant_id=$1::uuid")
        .bind(&tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM devices WHERE tenant_id IN ($1::uuid,$2::uuid)")
        .bind(&tenant)
        .bind(&other_tenant)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organizations WHERE id IN ($1::uuid,$2::uuid)")
        .bind(&tenant)
        .bind(&other_tenant)
        .execute(&pool)
        .await
        .unwrap();
    for user_id in user_ids {
        sqlx::query("DELETE FROM users WHERE id=$1::uuid")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
}
