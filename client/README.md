## Requirements

+ `gcc-aarch64-linux-gnu`

## Upload timeouts and progress

The optional `connect_timeout` and `request_timeout` settings are positive
numbers of seconds, defaulting to 10 and 30 respectively. Existing configs
automatically use these defaults. The connection timeout includes DNS and the
TLS handshake; the request timeout bounds the entire upload request.

An unsuccessful request keeps its readings in memory for the next attempt.
Sampling continues while an upload is pending. Buffered readings are still lost
on restart; persistent storage is separate future work. A timeout can occur after
the server has stored a request, so retries can produce duplicate readings until
the server supports deduplication.

With `RUST_LOG=info`, every sampling cycle and upload logs its start and result,
including reading counts and elapsed time. Upload failures identify timeouts and
the number of readings retained for retry. On the Pi, inspect them with:

```sh
journalctl -u rtherm -f
```

Repeated sampling logs without upload completion indicate a delivery problem.
A sampling start without completion indicates a provider read that has stalled.
The HTTP timeouts do not impose a deadline on sensor reads.
