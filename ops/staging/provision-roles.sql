\getenv auth_password TRACKER_AUTH_DB_PASSWORD
\getenv app_password TRACKER_APP_DB_PASSWORD
\getenv platform_password TRACKER_PLATFORM_DB_PASSWORD
\getenv mailer_password TRACKER_MAILER_DB_PASSWORD
\getenv retention_password TRACKER_RETENTION_DB_PASSWORD

SELECT format('CREATE ROLE gps_http_auth LOGIN INHERIT NOBYPASSRLS PASSWORD %L', :'auth_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_http_auth') \gexec
ALTER ROLE gps_http_auth LOGIN INHERIT NOBYPASSRLS PASSWORD :'auth_password';
GRANT gps_tracking_auth TO gps_http_auth;

SELECT format('CREATE ROLE gps_http_app LOGIN INHERIT NOBYPASSRLS PASSWORD %L', :'app_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_http_app') \gexec
ALTER ROLE gps_http_app LOGIN INHERIT NOBYPASSRLS PASSWORD :'app_password';
GRANT gps_tracking_app TO gps_http_app;

SELECT format('CREATE ROLE gps_http_platform LOGIN INHERIT NOBYPASSRLS PASSWORD %L', :'platform_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_http_platform') \gexec
ALTER ROLE gps_http_platform LOGIN INHERIT NOBYPASSRLS PASSWORD :'platform_password';
GRANT gps_tracking_platform_provisioner TO gps_http_platform;

SELECT format('CREATE ROLE gps_http_mailer LOGIN INHERIT NOBYPASSRLS PASSWORD %L', :'mailer_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_http_mailer') \gexec
ALTER ROLE gps_http_mailer LOGIN INHERIT NOBYPASSRLS PASSWORD :'mailer_password';
GRANT gps_tracking_mailer TO gps_http_mailer;

SELECT format('CREATE ROLE gps_http_retention LOGIN INHERIT NOBYPASSRLS PASSWORD %L', :'retention_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gps_http_retention') \gexec
ALTER ROLE gps_http_retention LOGIN INHERIT NOBYPASSRLS PASSWORD :'retention_password';
GRANT gps_tracking_retention_worker TO gps_http_retention;
