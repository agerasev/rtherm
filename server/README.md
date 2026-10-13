# rtherm-server

## Telegram access

Set an explicit numeric user allowlist alongside the bot token:

```toml
[telegram]
token = "..."
allowed_user_ids = [123456789]
```

An omitted or empty allowlist denies everyone. Only private messages from an
allowed user in their own chat are accepted. Groups, channels, missing senders,
and bot senders are ignored. Every outgoing reply and alert is checked too, so
previously saved subscriptions cannot bypass a changed allowlist. Unauthorized
subscriptions remain stored but receive no messages. Restart the server after
changing the list. Use stable numeric IDs verified with Telegram, rather than
usernames, which can change.

## Device dashboard

The dashboard groups sensor subcards inside device cards. Map the exact channel
IDs reported by clients to readable labels:

```toml
[[dashboard.devices]]
name = "Berezki"
[dashboard.devices.sensors]
BerezkiWater = "Water"
BerezkiAir = "Air"

[[dashboard.devices]]
name = "Workshop"
[dashboard.devices.sensors]
WorkshopAir = "Air"
```

Configured sensors remain visible when readings are absent. Unmapped channels
appear under **Unassigned device**. Duplicate mappings and empty labels are
rejected. The UI shows reading age, reporting/stale status, selected-range
statistics, and interactive 1/6/24-hour charts. A reading becomes stale after
four minutes; this describes sensor freshness separately from server connectivity.
Charts preserve extrema, cap each sensor at 720 plotted points, and leave gaps
across outages. Hover or use keyboard arrows to inspect plotted readings.

The server restores up to 24 hours of recent readings from its configured database
at startup (at most 200,000 readings in total); ongoing history retains up to
20,000 readings per channel. With no database, history starts with the process.
The existing Grafana link provides the longer archive. Dashboard reads do not
wait for recipient delivery, and failed browser requests retain the last displayed
data, time out after eight seconds, and retry automatically.

`GET /dashboard?hours=1|6|24` returns device mappings, latest readings, range
statistics, and bounded chart points. The existing `/summary` API remains available.
These HTTP endpoints keep the existing public access policy; the allowlist applies
to Telegram only.

Checks: `cargo test --locked -p rtherm-server` and
`node server/tests/ui.test.mjs` (from the repository root).

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
