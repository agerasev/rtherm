#![cfg(feature = "dummy")]

use rtherm_common::{ChannelId, ProvideRequest};
use std::{
    path::PathBuf,
    process::Stdio,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    task::JoinHandle,
    time::timeout,
};

struct ConfigDir(PathBuf);

impl Drop for ConfigDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn start_client(
    server: &str,
    connect_timeout: f64,
    request_timeout: f64,
) -> (Child, JoinHandle<String>, ConfigDir) {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    let dir = ConfigDir(std::env::temp_dir().join(format!(
        "rtherm-upload-test-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(&dir.0).unwrap();
    let config = dir.0.join("client.toml");
    std::fs::write(
        &config,
        format!(
            r#"
prefix = "Test"
server = "{server}"
period = 0.05
connect_timeout = {connect_timeout}
request_timeout = {request_timeout}
retry_interval = 0.05
providers = ["dummy"]
name_map = {{}}
"#
        ),
    )
    .unwrap();
    let (child, logs) = restart_client(&dir);
    (child, logs, dir)
}

fn restart_client(dir: &ConfigDir) -> (Child, JoinHandle<String>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rtherm-client"))
        .arg(dir.0.join("client.toml"))
        .current_dir(&dir.0)
        .env("RUST_LOG", "info")
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let logs = tokio::spawn(async move {
        let mut logs = String::new();
        stderr.read_to_string(&mut logs).await.unwrap();
        logs
    });
    (child, logs)
}

async fn stop_client(mut child: Child, logs: JoinHandle<String>) -> String {
    child.kill().await.unwrap();
    logs.await.unwrap()
}

async fn receive_request(listener: &TcpListener) -> (TcpStream, ProvideRequest) {
    timeout(Duration::from_secs(5), async {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.starts_with("POST /provide HTTP/1.1"), "{line}");
        let mut length = None;
        loop {
            line.clear();
            assert!(reader.read_line(&mut line).await.unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = Some(value.trim().parse::<usize>().unwrap());
            }
        }
        let mut body = vec![0; length.unwrap()];
        reader.read_exact(&mut body).await.unwrap();
        (reader.into_inner(), serde_json::from_slice(&body).unwrap())
    })
    .await
    .expect("client stopped making upload requests")
}

async fn acknowledge(mut stream: TcpStream) {
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nAccepted")
        .await
        .unwrap();
    stream.shutdown().await.unwrap();
}

async fn open_buffer(dir: &ConfigDir) -> sqlx::SqliteConnection {
    use sqlx::Connection;
    sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(dir.0.join("rtherm-buffer.sqlite3")),
    )
    .await
    .unwrap()
}

async fn wait_for_count(conn: &mut sqlx::SqliteConnection, predicate: impl Fn(i64) -> bool) {
    timeout(Duration::from_secs(5), async {
        loop {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_readings")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
            if predicate(count) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("buffer count did not reach expected state");
}

#[tokio::test]
async fn outage_and_sigkill_preserve_queue_then_restart_drains_bounded_batches() {
    use sqlx::Connection;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (client, logs, dir) = start_client(
        &format!("http://{}", listener.local_addr().unwrap()),
        0.5,
        0.2,
    );
    let (mut rejected, first) = receive_request(&listener).await;
    let first_point = first.measurements.values().next().unwrap()[0];
    rejected
        .write_all(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    rejected.shutdown().await.unwrap();
    let (stalled, retry) = receive_request(&listener).await;
    assert!(retry
        .measurements
        .values()
        .flatten()
        .any(|p| p.time == first_point.time && p.value == first_point.value));

    // Sampling must commit to disk while the upload is stalled.
    let mut conn = open_buffer(&dir).await;
    wait_for_count(&mut conn, |n| n >= 8).await;
    let logs = stop_client(client, logs).await; // SIGKILL, no graceful flush.
    drop(stalled);
    assert!(logs.contains("503"), "{logs}");
    let expected: Vec<(String, f64, i64)> =
        sqlx::query_as("SELECT channel, value, time_seconds FROM pending_readings ORDER BY id")
            .fetch_all(&mut conn)
            .await
            .unwrap();
    assert!(expected.len() >= 8);
    conn.close().await.unwrap();
    // Discard any already queued HTTP connection from the killed process.
    while timeout(Duration::from_millis(50), listener.accept())
        .await
        .is_ok()
    {}

    let config = dir.0.join("client.toml");
    let text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("providers = [\"dummy\"]", "providers = []")
        .replace("period = 0.05", "period = 3600");
    std::fs::write(&config, format!("{text}\nbatch_size = 3\n")).unwrap();
    let (client, logs) = restart_client(&dir);
    let mut received = Vec::new();
    while received.len() < expected.len() {
        let (stream, request) = receive_request(&listener).await;
        let count = request.measurements.values().map(Vec::len).sum::<usize>();
        assert!(
            (1..=3).contains(&count),
            "unbounded recovery batch: {count}"
        );
        for (channel, points) in request.measurements {
            for point in points {
                received.push((
                    channel.to_string(),
                    point.value,
                    point
                        .time
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                ));
            }
        }
        acknowledge(stream).await;
    }
    assert_eq!(
        received, expected,
        "restart lost or duplicated queued readings"
    );
    let mut conn = open_buffer(&dir).await;
    wait_for_count(&mut conn, |n| n == 0).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&mut conn)
            .await
            .unwrap(),
        "ok"
    );
    conn.close().await.unwrap();
    stop_client(client, logs).await;

    // A second restart must not replay rows that were acknowledged and deleted.
    let (client, logs) = restart_client(&dir);
    assert!(timeout(Duration::from_millis(300), listener.accept())
        .await
        .is_err());
    let logs = stop_client(client, logs).await;
    assert!(logs.contains("0 readings pending"), "{logs}");
}

#[tokio::test]
async fn redirect_is_not_an_acknowledgement() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (client, logs, _dir) = start_client(
        &format!("http://{}", listener.local_addr().unwrap()),
        0.5,
        1.0,
    );
    let (mut stream, first) = receive_request(&listener).await;
    let point = first.measurements.values().next().unwrap()[0];
    stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /login\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    stream.shutdown().await.unwrap();
    let (stream, retry) = receive_request(&listener).await;
    assert!(retry
        .measurements
        .values()
        .flatten()
        .any(|p| p.time == point.time && p.value == point.value));
    acknowledge(stream).await;
    let logs = stop_client(client, logs).await;
    assert!(logs.contains("Upload rejected with HTTP 302"), "{logs}");
}

#[tokio::test]
async fn stalled_response_times_out_and_retries_buffered_readings() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (client, logs, _config) = start_client(
        &format!("http://{}", listener.local_addr().unwrap()),
        0.5,
        1.0,
    );
    // Keep the first connection open, but never return its HTTP response.
    let (_stalled, first) = receive_request(&listener).await;
    let channel = ChannelId::try_from("Testdummy").unwrap();
    let first_point = first.measurements[&channel].first().copied().unwrap();
    let (retry, second) = receive_request(&listener).await;
    let points = &second.measurements[&channel];
    assert!(
        points.len() > 1,
        "sampling stopped during the stalled upload"
    );
    assert!(
        points
            .iter()
            .any(|p| p.time == first_point.time && p.value == first_point.value),
        "the timed-out request's reading was lost"
    );
    acknowledge(retry).await;

    let (next, third) = receive_request(&listener).await;
    assert!(
        third
            .measurements
            .values()
            .flatten()
            .all(|p| p.time != first_point.time || p.value != first_point.value),
        "acknowledged readings were not removed from the buffer"
    );
    acknowledge(next).await;
    let logs = stop_client(client, logs).await;
    for message in [
        "Sampling started",
        "Sampling completed:",
        "Upload started:",
        "Upload failed after",
        "timeout=true",
        "readings retained for retry",
        "Upload completed:",
    ] {
        assert!(logs.contains(message), "missing {message:?}:\n{logs}");
    }
}

#[tokio::test]
async fn stalled_tls_handshake_obeys_connection_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (client, logs, _config) = start_client(
        &format!("https://{}", listener.local_addr().unwrap()),
        0.3,
        10.0,
    );
    timeout(Duration::from_secs(3), async {
        let (mut stalled, _) = listener.accept().await.unwrap();
        let mut hello = [0; 1024];
        assert!(stalled.read(&mut hello).await.unwrap() > 0);
        // A new connection must be attempted before the 10-second total timeout.
        let (_retry, _) = listener.accept().await.unwrap();
        drop(stalled);
    })
    .await
    .expect("TLS handshake did not obey the connection timeout");
    let logs = stop_client(client, logs).await;
    assert!(logs.contains("timeout=true"), "{logs}");
    assert!(logs.contains("readings retained for retry"), "{logs}");
}
