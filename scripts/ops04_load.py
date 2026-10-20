#!/usr/bin/env python3
"""Reproducible synthetic OPS-04 load run against an explicitly disposable DB."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
import secrets
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTAINER = "gps-ops04-db"
PORT = 18080
ORG_IDS = [str(uuid.uuid5(uuid.NAMESPACE_DNS, f"ops04-org-{i}")) for i in range(2)]
DEVICE_IDS = [str(uuid.uuid5(uuid.NAMESPACE_DNS, f"ops04-device-{i}")) for i in range(100)]
TARGET_IDS = [str(uuid.uuid5(uuid.NAMESPACE_DNS, f"ops04-target-{i}")) for i in range(100)]
USER_IDS = [str(uuid.uuid5(uuid.NAMESPACE_DNS, f"ops04-user-{i}")) for i in range(2)]
MEMBERSHIP_IDS = [str(uuid.uuid5(uuid.NAMESPACE_DNS, f"ops04-membership-{i}")) for i in range(2)]
SESSION_IDS = [str(uuid.uuid5(uuid.NAMESPACE_DNS, f"ops04-session-{i}")) for i in range(100)]
DEVICE_TOKENS: list[str] = []
APP_TOKENS: list[str] = []


def docker_psql(sql: str) -> str:
    result = subprocess.run(
        ["docker", "exec", "-i", CONTAINER, "psql", "-X", "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", "tracker_test", "-At"],
        input=sql,
        text=True,
        capture_output=True,
    )
    if result.returncode:
        raise RuntimeError(f"disposable database command failed ({result.returncode}): {result.stderr.strip()}")
    return result.stdout.strip()


def execute(sql: str) -> None:
    docker_psql(sql)


def provision_and_seed() -> tuple[int, int, int, float]:
    global DEVICE_TOKENS, APP_TOKENS
    DEVICE_TOKENS = ["gps_live_" + secrets.token_urlsafe(32) for _ in range(100)]
    APP_TOKENS = ["gps_app_" + secrets.token_urlsafe(32) for _ in range(2)]
    existing = int(docker_psql(
        "SELECT count(*) FROM organizations WHERE id=ANY(ARRAY[%s]::uuid[]);"
        % ",".join("'%s'::uuid" % org for org in ORG_IDS)
    ))
    if existing:
        raise RuntimeError("OPS-04 fixture organizations already exist; use a newly created disposable database (the harness never deletes benchmark data)")
    values: list[str] = []
    for i, org in enumerate(ORG_IDS):
        values.append(
            "INSERT INTO organizations(id,name) VALUES('%s'::uuid,'OPS-04 synthetic tenant %d');" % (org, i + 1)
        )
        values.append(
            "INSERT INTO users(id,email_canonical,display_name,password_hash,status) VALUES('%s'::uuid,'ops04-%d@example.invalid','Synthetic OPS-04','%s','active');"
            % (USER_IDS[i], i, "$argon2id$v=19$m=65536,t=3,p=1$synthetic$synthetic-fixture-only")
        )
        values.append(
            "INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES('%s'::uuid,'%s'::uuid,'%s'::uuid,'organization_admin','active');"
            % (MEMBERSHIP_IDS[i], org, USER_IDS[i])
        )
    for i, device in enumerate(DEVICE_IDS):
        tenant_idx = i // 50
        token_hash = hashlib.sha256(DEVICE_TOKENS[i].encode()).hexdigest()
        values.extend(
            [
                "INSERT INTO targets(id,tenant_id,name) VALUES('%s'::uuid,'%s'::uuid,'OPS-04 target %03d');" % (TARGET_IDS[i], ORG_IDS[tenant_idx], i),
                "INSERT INTO devices(id,tenant_id,name,protocol,identifier,ingestion_policy) VALUES('%s'::uuid,'%s'::uuid,'OPS-04 device %03d','http-json','ops04-%03d','session_only');" % (device, ORG_IDS[tenant_idx], i, i),
                "INSERT INTO device_credentials(id,tenant_id,device_id,token_hash) VALUES(gen_random_uuid(),'%s'::uuid,'%s'::uuid,decode('%s','hex'));" % (ORG_IDS[tenant_idx], device, token_hash),
                "INSERT INTO tracking_sessions(id,tenant_id,target_id,device_id,started_at) VALUES('%s'::uuid,'%s'::uuid,'%s'::uuid,'%s'::uuid,clock_timestamp()-interval '1 day');" % (SESSION_IDS[i], ORG_IDS[tenant_idx], TARGET_IDS[i], device),
            ]
        )
    for i, token in enumerate(APP_TOKENS):
        tenant_devices = range(i * 50, (i + 1) * 50)
        token_id = str(uuid.uuid5(uuid.NAMESPACE_DNS, f"ops04-app-token-{i}"))
        values.append(
            "INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) VALUES('%s'::uuid,'%s'::uuid,'%s'::uuid,'OPS-04 synthetic read/session token',decode('%s','hex'),ARRAY['positions:read','devices:read','targets:read','sessions:read','sessions:write'],clock_timestamp()+interval '30 days');"
            % (token_id, ORG_IDS[i], USER_IDS[i], hashlib.sha256(token.encode()).hexdigest())
        )
        values.extend(
            "INSERT INTO application_token_devices(tenant_id,application_token_id,device_id) VALUES('%s'::uuid,'%s'::uuid,'%s'::uuid);" % (ORG_IDS[i], token_id, DEVICE_IDS[j])
            for j in tenant_devices
        )
        values.extend(
            "INSERT INTO application_token_targets(tenant_id,application_token_id,target_id) VALUES('%s'::uuid,'%s'::uuid,'%s'::uuid);" % (ORG_IDS[i], token_id, TARGET_IDS[j])
            for j in tenant_devices
        )
    execute("BEGIN;\n" + "\n".join(values) + "\nCOMMIT;")

    start = time.monotonic()
    execute(
        """
        INSERT INTO positions(tenant_id,device_id,message_id,payload_fingerprint,location,recorded_at)
        SELECT d.tenant_id,d.id,gen_random_uuid(),
               decode(md5(d.id::text||g.n::text)||md5(g.n::text||d.id::text),'hex'),
               ST_SetSRID(ST_MakePoint(110.84+mod(g.n,20)*0.00001,-6.8+mod(g.n,20)*0.00001),4326)::geography,
               clock_timestamp()-(g.n * interval '1 second')
        FROM devices d CROSS JOIN generate_series(1,10000) AS g(n)
        WHERE d.id = ANY(ARRAY[%s]::uuid[]);
        """ % ",".join("'%s'::uuid" % device for device in DEVICE_IDS)
    )
    seed_seconds = time.monotonic() - start
    total_count, synthetic_count, size = docker_psql(
        "SELECT count(*) FROM positions; "
        "SELECT count(*) FROM positions WHERE device_id=ANY(ARRAY[%s]::uuid[]); "
        "SELECT pg_total_relation_size('positions');"
        % ",".join("'%s'::uuid" % device for device in DEVICE_IDS)
    ).splitlines()[-3:]
    if int(synthetic_count) != 1_000_000:
        raise RuntimeError(f"expected 1,000,000 synthetic positions, found {synthetic_count}")
    execute("""
        CREATE ROLE ops04_auth LOGIN INHERIT PASSWORD 'ops04-auth-disposable';
        GRANT gps_tracking_auth TO ops04_auth;
        CREATE ROLE ops04_tracking LOGIN INHERIT PASSWORD 'ops04-tracking-disposable';
        GRANT gps_tracking_app TO ops04_tracking;
    """)
    return int(synthetic_count), int(total_count), int(size), seed_seconds


def request(method: str, path: str, token: str, tenant: str, body: dict | None = None) -> tuple[int, float, dict | None]:
    data = json.dumps(body or {}).encode()
    req = urllib.request.Request(
        f"http://127.0.0.1:{PORT}{path}",
        data=data if method != "GET" else None,
        method=method,
        headers={
            "Authorization": f"Bearer {token}",
            "X-Organization-Id": tenant,
            "Content-Type": "application/json",
        },
    )
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=20) as response:
            payload = response.read()
            return response.status, (time.perf_counter() - started) * 1000, json.loads(payload) if payload else None
    except urllib.error.HTTPError as error:
        payload = error.read()
        try:
            decoded = json.loads(payload) if payload else None
        except json.JSONDecodeError:
            decoded = None
        return error.code, (time.perf_counter() - started) * 1000, decoded


def percentile(values: list[float], percent: float) -> float:
    values = sorted(values)
    if not values:
        return 0.0
    return values[min(len(values) - 1, int((len(values) - 1) * percent + 0.999999))]


def report_measurement(name: str, result: list[tuple[int, float, dict | None]], seconds: float) -> dict:
    latency = [r[1] for r in result]
    status_counts: dict[str, int] = {}
    for status, _, _ in result:
        status_counts[str(status)] = status_counts.get(str(status), 0) + 1
    record = {
        "scenario": name,
        "requests": len(result),
        "elapsed_seconds": round(seconds, 3),
        "throughput_requests_per_second": round(len(result) / seconds, 2) if seconds else 0,
        "latency_ms_p50": round(percentile(latency, 0.50), 2),
        "latency_ms_p95": round(percentile(latency, 0.95), 2),
        "latency_ms_max": round(max(latency, default=0), 2),
        "http_statuses": status_counts,
        "http_200": status_counts.get("200", 0),
        "non_200": sum(count for status, count in status_counts.items() if status != "200"),
    }
    print(json.dumps(record, sort_keys=True))
    return record


def run_scenario(workers: int, jobs, label: str, expected_points: int) -> dict:
    started = time.monotonic()
    results: list[tuple[int, float, dict | None]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        futures = [pool.submit(fn) for fn in jobs]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    elapsed = time.monotonic() - started
    record = report_measurement(label, results, elapsed)
    record["expected_points"] = expected_points
    record["stored_points"] = sum(
        item[2].get("summary", {}).get("stored", 0)
        for item in results
        if item[2] and isinstance(item[2], dict)
    )
    record["rows_returned"] = sum(
        len(item[2].get("data", []))
        for item in results
        if item[2] and isinstance(item[2], dict) and isinstance(item[2].get("data"), list)
    )
    return record


def run_paced_scenario(workers: int, jobs, label: str, requests_per_second: int, expected_points: int) -> dict:
    started = time.monotonic()
    results: list[tuple[int, float, dict | None]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        futures = []
        for index, job in enumerate(jobs):
            deadline = started + index / requests_per_second
            delay = deadline - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            futures.append(pool.submit(job))
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    elapsed = time.monotonic() - started
    record = report_measurement(label, results, elapsed)
    record["target_requests_per_second"] = requests_per_second
    record["expected_points"] = expected_points
    record["stored_points"] = sum(
        item[2].get("summary", {}).get("stored", 0)
        for item in results
        if item[2] and isinstance(item[2], dict)
    )
    record["rows_returned"] = sum(
        len(item[2].get("data", []))
        for item in results
        if item[2] and isinstance(item[2], dict) and isinstance(item[2].get("data"), list)
    )
    record["actual_points_per_second"] = round(record["stored_points"] / elapsed, 2) if elapsed else 0
    return record


def wait_for_server(process: subprocess.Popen, log) -> None:
    for _ in range(90):
        if process.poll() is not None:
            log.flush()
            log.seek(0)
            raise RuntimeError(f"server exited with status {process.returncode}: {log.read().decode(errors='replace')[-2000:]}")
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{PORT}/readyz", timeout=1) as response:
                if response.status == 200:
                    return
        except Exception:
            time.sleep(1)
    raise TimeoutError("server did not become ready")


def server_environment() -> dict[str, str]:
    env = os.environ.copy()
    auth_db_url = "postgres://ops04_auth:ops04-auth-disposable@127.0.0.1:54331/tracker_test?sslmode=disable"
    tracking_db_url = "postgres://ops04_tracking:ops04-tracking-disposable@127.0.0.1:54331/tracker_test?sslmode=disable"
    env.update({
        "DATABASE_URL": auth_db_url,
        "TRACKING_DATABASE_URL": tracking_db_url,
        "BIND_ADDR": f"127.0.0.1:{PORT}",
        "MFA_ENCRYPTION_KEY": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "OUTBOX_ENCRYPTION_KEY": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "RUST_LOG": "server=warn",
    })
    return env


def run_history_only() -> None:
    global APP_TOKENS
    APP_TOKENS = ["gps_app_" + secrets.token_urlsafe(32) for _ in range(2)]
    statements = ["BEGIN;"]
    for org_idx in range(2):
        token_id = str(uuid.uuid4())
        statements.append(
            "INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) "
            "VALUES('%s'::uuid,'%s'::uuid,'%s'::uuid,'OPS-04 history-only token',decode('%s','hex'),"
            "ARRAY['positions:read','devices:read','targets:read'],clock_timestamp()+interval '30 days');"
            % (token_id, ORG_IDS[org_idx], USER_IDS[org_idx], hashlib.sha256(APP_TOKENS[org_idx].encode()).hexdigest())
        )
        for device_idx in range(org_idx * 50, org_idx * 50 + 15):
            statements.append(
                "INSERT INTO application_token_devices(tenant_id,application_token_id,device_id) "
                "VALUES('%s'::uuid,'%s'::uuid,'%s'::uuid);" % (ORG_IDS[org_idx], token_id, DEVICE_IDS[device_idx])
            )
    statements.append("COMMIT;")
    execute("\n".join(statements))
    server_log = tempfile.TemporaryFile()
    server = subprocess.Popen(["cargo", "run", "--locked", "-p", "server", "--bin", "server"], cwd=ROOT, env=server_environment(), stdout=subprocess.DEVNULL, stderr=server_log)
    try:
        wait_for_server(server, server_log)
        jobs = [
            lambda idx=idx, org_idx=org_idx: request("GET", f"/v1/devices/{DEVICE_IDS[idx]}/positions?limit=100", APP_TOKENS[org_idx], ORG_IDS[org_idx])
            for org_idx in range(2)
            for idx in range(org_idx * 50, org_idx * 50 + 15)
        ]
        result = run_scenario(10, jobs, "history_page_30_samples_after_rate_limit", 0)
        if result["non_200"] or result["rows_returned"] != 3000:
            raise RuntimeError("history-only checks failed: expected 30 HTTP 200 pages and 3,000 rows")
        if result["latency_ms_p95"] > 500:
            raise RuntimeError("history page p95 exceeded 500ms target")
    finally:
        server.send_signal(signal.SIGTERM)
        try:
            server.wait(timeout=15)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait(timeout=5)
        server_log.close()


def run() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--steady-seconds", type=int, default=120)
    parser.add_argument("--history-only", action="store_true", help="Measure 30 history pages against an existing completed OPS-04 seed; does not delete or reseed data.")
    args = parser.parse_args()
    if args.steady_seconds < 10:
        parser.error("--steady-seconds must be at least 10")

    if args.history_only:
        run_history_only()
        return

    synthetic_count, total_count, size, seed_seconds = provision_and_seed()
    print(json.dumps({"scenario": "seed", "synthetic_positions": synthetic_count, "total_positions_after_seed": total_count, "positions_relation_bytes": size, "seed_elapsed_seconds": round(seed_seconds, 3), "steady_seconds": args.steady_seconds}, sort_keys=True))

    env = os.environ.copy()
    auth_db_url = "postgres://ops04_auth:ops04-auth-disposable@127.0.0.1:54331/tracker_test?sslmode=disable"
    tracking_db_url = "postgres://ops04_tracking:ops04-tracking-disposable@127.0.0.1:54331/tracker_test?sslmode=disable"
    env.update({
        "DATABASE_URL": auth_db_url,
        "TRACKING_DATABASE_URL": tracking_db_url,
        "BIND_ADDR": f"127.0.0.1:{PORT}",
        "MFA_ENCRYPTION_KEY": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "OUTBOX_ENCRYPTION_KEY": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "RUST_LOG": "server=warn",
    })
    server_log = tempfile.TemporaryFile()
    server = subprocess.Popen(["cargo", "run", "--locked", "-p", "server", "--bin", "server"], cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=server_log)
    try:
        wait_for_server(server, server_log)
        report: list[dict] = []
        before = int(docker_psql("SELECT count(*) FROM positions;"))

        heartbeat_jobs = [
            lambda i=i: request("POST", "/v1/ingest/heartbeat", DEVICE_TOKENS[i], ORG_IDS[i // 50], {})
            for i in range(100)
        ]
        report.append(run_paced_scenario(10, heartbeat_jobs, "idle_heartbeat_100_devices", 10, 0))
        after = int(docker_psql("SELECT count(*) FROM positions;"))
        report[-1]["position_rows_before"] = before
        report[-1]["position_rows_after"] = after
        report[-1]["heartbeat_added_positions"] = after - before

        steady_jobs = []
        for i in range(args.steady_seconds * 10):
            due = time.monotonic() + i / 10
            device_idx = i % 100
            def steady_job(idx=device_idx, deadline=due, n=i):
                delay = deadline - time.monotonic()
                if delay > 0:
                    time.sleep(delay)
                body = {"device_id": DEVICE_IDS[idx], "points": [{"message_id": f"ops04-steady-{n}", "latitude": -6.81, "longitude": 110.84, "recorded_at": (dt.datetime.now(dt.timezone.utc) - dt.timedelta(seconds=2)).isoformat()}]}
                return request("POST", "/v1/ingest/positions", DEVICE_TOKENS[idx], ORG_IDS[idx // 50], body)
            steady_jobs.append(steady_job)
        report.append(run_scenario(32, steady_jobs, "steady_10_points_per_second", args.steady_seconds * 10))

        burst_jobs = []
        for second in range(60):
            for idx in range(100):
                msg = f"ops04-burst-{second}-{idx}"
                point = {"message_id": msg, "latitude": -6.81, "longitude": 110.84, "recorded_at": (dt.datetime.now(dt.timezone.utc) - dt.timedelta(seconds=2)).isoformat()}
                body = {"device_id": DEVICE_IDS[idx], "points": [point]}
                burst_jobs.append(lambda idx=idx, body=body: request("POST", "/v1/ingest/positions", DEVICE_TOKENS[idx], ORG_IDS[idx // 50], body))
        burst_record = run_paced_scenario(10, burst_jobs, "burst_100_points_per_second_60s", 100, 6000)
        report.append(burst_record)

        # Each device has a 1 request/second ingestion bucket; let its final burst request refill.
        time.sleep(1.1)

        backfill_jobs = []
        backfill_time = dt.datetime.now(dt.timezone.utc) - dt.timedelta(hours=6)
        for batch_no in range(10):
            idx = batch_no
            points = [
                {"message_id": f"ops04-backfill-{batch_no}-{j}", "latitude": -6.81, "longitude": 110.84, "recorded_at": (backfill_time + dt.timedelta(seconds=j)).isoformat()}
                for j in range(100)
            ]
            backfill_jobs.append(lambda idx=idx, points=points: request("POST", "/v1/ingest/positions", DEVICE_TOKENS[idx], ORG_IDS[idx // 50], {"device_id": DEVICE_IDS[idx], "points": points}))
        backfill_record = run_scenario(10, backfill_jobs, "backfill_10_batches_x_100_points", 1000)
        report.append(backfill_record)
        if "stored_points" not in backfill_record:
            backfill_record["stored_points"] = 0

        history_jobs = []
        for org_idx in range(2):
            for offset in range(15):
                idx = org_idx * 50 + offset
                history_jobs.append(lambda idx=idx, org_idx=org_idx: request("GET", f"/v1/devices/{DEVICE_IDS[idx]}/positions?limit=100", APP_TOKENS[org_idx], ORG_IDS[org_idx]))
        history_record = run_scenario(10, history_jobs, "history_page_50_samples", 0)
        report.append(history_record)

        time.sleep(1.1)

        concurrent_records = []
        requests = []
        for idx in range(10):
            requests.append((idx, "ingest"))
            requests.append((idx, "end"))
        started = time.monotonic()
        race_results = []
        def race_call(idx: int, kind: str):
            if kind == "ingest":
                point = {"message_id": f"ops04-session-race-{idx}", "latitude": -6.81, "longitude": 110.84, "recorded_at": dt.datetime.now(dt.timezone.utc).isoformat()}
                return kind, request("POST", "/v1/ingest/positions", DEVICE_TOKENS[idx], ORG_IDS[idx // 50], {"device_id": DEVICE_IDS[idx], "points": [point]})
            return kind, request("POST", f"/v1/sessions/{SESSION_IDS[idx]}/end", APP_TOKENS[idx // 50], ORG_IDS[idx // 50], {})
        with concurrent.futures.ThreadPoolExecutor(max_workers=20) as pool:
            futures = [pool.submit(race_call, idx, kind) for idx, kind in requests]
            race_results = [future.result() for future in concurrent.futures.as_completed(futures)]
        race_elapsed = time.monotonic() - started
        for kind in ("ingest", "end"):
            report.append(report_measurement(f"concurrent_session_end_vs_{kind}", [row[1] for row in race_results if row[0] == kind], race_elapsed))

        query_plan = json.loads(docker_psql(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT id,recorded_at FROM positions "
            "WHERE tenant_id='%s'::uuid AND device_id='%s'::uuid "
            "AND recorded_at>=clock_timestamp()-interval '24 hours' "
            "AND recorded_at<clock_timestamp() AND expires_at>clock_timestamp() "
            "ORDER BY recorded_at DESC,id DESC LIMIT 101;" % (ORG_IDS[0], DEVICE_IDS[0])
        ))[0]
        print(json.dumps({"scenario": "history_query_plan", "plan": query_plan}, sort_keys=True))

        final_count = int(docker_psql("SELECT count(*) FROM positions;"))
        total_size = int(docker_psql("SELECT pg_total_relation_size('positions');"))
        row_bytes = total_size / final_count if final_count else 0
        annual_points = 10 * 365 * 24 * 60 * 60
        print(json.dumps({
            "scenario": "storage_forecast",
            "positions_rows": final_count,
            "positions_total_relation_bytes": total_size,
            "observed_bytes_per_position_including_indexes": round(row_bytes, 2),
            "assumption": "10 accepted positions/second sustained for 365 days; synthetic fixture distribution and current indexes; excludes WAL, backups, replicas, vacuum/headroom",
            "projected_one_year_position_relation_bytes": round(row_bytes * annual_points),
            "projected_one_year_position_relation_gib": round(row_bytes * annual_points / (1024 ** 3), 2),
        }, sort_keys=True))
        print(json.dumps({"scenario": "summary", "http_results": report}, sort_keys=True))
        failures = []
        for result in report:
            if result["scenario"].startswith(("steady_", "burst_", "backfill_", "idle_heartbeat_", "history_page_")) and result["non_200"]:
                failures.append(f"{result['scenario']} had {result['non_200']} non-200 responses")
            if result["scenario"].startswith("concurrent_session_end_vs_") and result["non_200"]:
                failures.append(f"{result['scenario']} had {result['non_200']} non-200 responses")
        if report[0]["heartbeat_added_positions"] != 0:
            failures.append("heartbeat inserted positions")
        if report[1]["stored_points"] != args.steady_seconds * 10:
            failures.append("steady ingestion stored count mismatch")
        burst = next(item for item in report if item["scenario"] == "burst_100_points_per_second_60s")
        if burst["stored_points"] != 6000:
            failures.append("burst ingestion stored count mismatch")
        if backfill_record["stored_points"] != 1000:
            failures.append("backfill stored count mismatch")
        if history_record["rows_returned"] != 3000:
            failures.append("history page returned count mismatch")
        if report[1]["latency_ms_p95"] > 300:
            failures.append("steady ingestion p95 exceeded 300ms target")
        if history_record["latency_ms_p95"] > 500:
            failures.append("history page p95 exceeded 500ms target")
        if failures:
            raise RuntimeError("scenario acceptance checks failed: " + "; ".join(failures))
    finally:
        server.send_signal(signal.SIGTERM)
        try:
            server.wait(timeout=15)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait(timeout=5)
        server_log.close()


if __name__ == "__main__":
    try:
        run()
    except Exception as error:
        print(f"OPS-04 benchmark failed: {type(error).__name__}: {error}", file=sys.stderr)
        raise
