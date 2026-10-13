## Requirements

+ `gcc-aarch64-linux-gnu`

## Upload timeouts and progress

The optional `connect_timeout` and `request_timeout` settings are positive
numbers of seconds, defaulting to 10 and 30 respectively. Existing configs
automatically use these defaults. The connection timeout includes DNS and the
TLS handshake; the request timeout bounds the entire upload request.

Sampling continues while an upload is pending. Failed uploads retry with exponential
backoff: `retry_interval` seconds initially (default 5), doubling up to 60 seconds
(or the initial interval, if larger). Successful uploads reset the delay.

## Persistent buffer

Every sampling cycle is committed to SQLite before it can be uploaded. The default
`buffer_path` is `rtherm-buffer.sqlite3`, relative to the **working directory**,
not the config file. Set an absolute path when managing a service. The parent
directory must already exist and be writable by the service user. Existing
configs automatically enable persistence with the default path.

The Pi uses `/opt/rtherm/rtherm-buffer.sqlite3`. SQLite is bundled in the client
binary; no separate SQLite service or package is required.

The queue uses WAL mode with FULL synchronization and atomic sampling transactions.
Committed readings survive client restarts, including forced termination. On
startup, the uploader drains the queue even if no sensors produce new readings.
Uploads contain at most `batch_size` readings (default 500, allowed 1–1000).
Only a successful HTTP 2xx acknowledges a batch; errors, timeouts, and redirects
retain it. Readings collected during that upload remain queued for a later batch.
There is one uploader and no database transaction is held during a network request.

If a write fails (for example, a full disk), the client logs the error and pauses
sampling while retrying the current cycle. That uncommitted cycle remains in RAM
and cannot survive a restart until a write succeeds. Previously committed rows
remain queued. Startup fails if the buffer cannot be opened; there is no silent
fallback to memory. There is no automatic expiry or size cap that discards old
readings. Monitor free space during long outages; SQLite reuses freed pages after
delivery but does not automatically shrink the database file.

Keep the buffer path stable across deployments and run only one client per buffer.
For a backup, use SQLite's backup API, or stop the client before copying its database
and any `-wal`/`-shm` sidecars. Do not delete sidecars from a running client.

Delivery uses the existing server protocol. A lost acknowledgement or a crash
between acknowledgement and local deletion can cause duplicate delivery. Also,
the current server can return HTTP 200 after a database write error; local buffering
cannot detect that false acknowledgement. Server-side transactional acknowledgement
and deduplication are still needed for an end-to-end durable delivery guarantee.

## Logs and checks

With `RUST_LOG=info`, every sampling cycle and upload logs its start and result,
including reading counts and elapsed time. Upload failures identify timeouts and
the number of readings retained for retry. Startup reports pending readings;
`Buffered ... readings in SQLite` confirms a committed cycle, and upload completion
confirms removal of the acknowledged batch. On the Pi, inspect them with:

```sh
journalctl -u rtherm -f
```

Repeated sampling logs without upload completion indicate a delivery problem.
A sampling start without completion indicates a provider read that has stalled.
The HTTP timeouts do not impose a deadline on sensor reads.

Run the client tests with `cargo test --locked -p rtherm-client`. They cover stalled
HTTP/TLS connections, rejection and redirects, writes during upload, forced
termination and recovery, bounded backlog draining without new samples, persistent
acknowledgement cleanup, and transaction rollback on storage errors.
