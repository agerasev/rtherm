# rtherm-server

## Telegram behind an HTTP proxy

If the server cannot reach Telegram directly, configure an HTTPS proxy in
`docker/.env`. For example, to use Squid running on the Docker host:

```dotenv
RTHERM_HTTPS_PROXY=http://host.docker.internal:3128
```

The proxy must accept connections from the container's Docker network. HTTPS
uses a CONNECT tunnel, with TLS certificate verification still enabled. The
client library supports the standard [HTTPS_PROXY environment variable](https://docs.rs/reqwest/0.12.9/reqwest/#proxies).

Apply the environment change from `server/docker`:

```sh
docker compose up -d --no-deps rtherm-server
```

If `RTHERM_HTTPS_PROXY` is unset, the server connects directly. The `.env` file
is ignored by Git.

## Grafana

+ Connect to `localhost:4101`
+ Enter `admin/admin`
+ Change password and optinally username

+ Go to Data sources and add a new one
+ Add PostgreSQL datasource
  + Name: `postgresql`
  + Url: `postgres`
  + Database name: `rtherm`
  + Username: `rtherm`
  + Password: `rtherm`
  + TLS/SSL Mode: `disable`

+ Go to Dashboards and create a new one
  + Data source: `postgresql`
  + Format: Time series
  + Columns:
    + `value`
    + `time`
    + `channel_id`
  + Order by: `time`, limit: blank
  + Save
