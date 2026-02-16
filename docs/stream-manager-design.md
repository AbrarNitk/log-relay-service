# StreamManager Design (Revised)

## Overview
The `StreamManager` is the core component of the Log Relay Service responsible for managing NATS consumers and broadcasting logs to multiple web clients. It ensures efficient resource usage by sharing a single NATS consumer per `run_id` across multiple SSE connections via a `broadcast` channel and `Arc`/`Weak` reference counting.

## Key Features
1.  **Shared Consumers**: Multiple clients requesting the same `run_id` share one underlying NATS consumer — N SSE connections → 1 NATS consumer → 1 TCP virtual channel on the single NATS connection.
2.  **Race-Free Creation**: `DashMap::entry()` API guarantees at most one consumer is created per `run_id`, even under concurrent requests.
3.  **Automatic Cleanup**: Resources (NATS consumers, background tasks) are automatically cleaned up when the last client disconnects via `Drop`-triggered `CancellationToken`.
4.  **Idle Timeout**: Streams automatically close if no logs are received for a configurable duration (default 5 minutes).
5.  **Back-Pressure Handling**: Bounded `broadcast` channel (default 4096) protects against OOM; lagging clients receive a warning event.
6.  **External Control**: `DELETE /streams/{run_id}` API to forcefully stop a stream.
7.  **Zero-Copy Path**: `Bytes` from NATS are broadcast directly; UTF-8 conversion happens only at the SSE serialization boundary.

## Architecture

### Data Structures

```rust
pub struct StreamManager {
    /// Map of run_id -> Weak<SharedStream>.
    /// Weak ensures the map alone doesn't keep the stream alive.
    streams: Arc<DashMap<String, Weak<SharedStream>>>,

    /// JetStream context — one per NATS TCP connection (shared across all consumers).
    js: jetstream::Context,

    /// Relay settings (idle timeout, broadcast capacity, etc.)
    config: Arc<RelaySettings>,
}

pub struct SharedStream {
    /// The run_id this stream is for.
    pub run_id: String,

    /// Bounded broadcast channel — sends raw `Bytes` from NATS to all SSE receivers.
    pub sender: broadcast::Sender<Bytes>,

    /// CancellationToken to signal the background NATS pump task to exit.
    pub shutdown: CancellationToken,

    /// Back-ref to the DashMap for self-removal on Drop.
    streams_map: Arc<DashMap<String, Weak<SharedStream>>>,
}
```

### Memory & Resource Model

```
  Single NATS TCP Connection
          │
          ├── Virtual Channel: Consumer for run_id "abc"
          │         │
          │         └── broadcast::Sender<Bytes> (cap: 4096)
          │               ├── Receiver → SSE Client 1
          │               ├── Receiver → SSE Client 2
          │               └── Receiver → SSE Client N
          │
          ├── Virtual Channel: Consumer for run_id "xyz"
          │         └── broadcast::Sender<Bytes>
          │               └── Receiver → SSE Client
          │
          └── Virtual Channel: Durable consumer for archiver
                    └── Batched → OpenObserve HTTP POST
```

- **One TCP connection** to NATS, multiplexed into N virtual channels.
- **One background tokio task** per active `run_id` (NATS pump).
- **One `broadcast` channel** per `run_id` — `Bytes` are reference-counted, not cloned per receiver.
- **Each SSE client** holds an `Arc<SharedStream>` — when the last `Arc` is dropped, `Drop` cancels the background task.

### Lifecycle Flow

#### 1. Client Connection (`get_or_create_stream`)

```rust
pub async fn get_or_create_stream(&self, run_id: &str) -> Arc<SharedStream> {
    // Fast path: try upgrade existing Weak (no write lock)
    if let Some(entry) = self.streams.get(run_id) {
        if let Some(arc) = entry.upgrade() {
            return arc;
        }
    }

    // Slow path: entry API — atomic insert-or-upgrade per shard
    let entry = self.streams.entry(run_id.to_string());
    match entry {
        dashmap::mapref::entry::Entry::Occupied(mut occ) => {
            if let Some(arc) = occ.get().upgrade() {
                return arc;
            }
            // Dead Weak — replace it
            let shared = Arc::new(self.create_shared_stream(run_id).await);
            occ.insert(Arc::downgrade(&shared));
            self.spawn_nats_pump(run_id, Arc::clone(&shared));
            shared
        }
        dashmap::mapref::entry::Entry::Vacant(vac) => {
            let shared = Arc::new(self.create_shared_stream(run_id).await);
            vac.insert(Arc::downgrade(&shared));
            self.spawn_nats_pump(run_id, Arc::clone(&shared));
            shared
        }
    }
}
```

**Why this is race-free**: `DashMap::entry()` holds the shard lock for the duration of the match arm. Two concurrent callers for the same `run_id` are serialized by the shard lock. The first creates; the second finds the `Weak` and upgrades.

#### 2. Background NATS Pump Task

```rust
fn spawn_nats_pump(&self, run_id: &str, shared: Arc<SharedStream>) {
    let config = Arc::clone(&self.config);
    let js = self.js.clone();
    let run_id = run_id.to_string();

    tokio::spawn(async move {
        let filter = format!("logs.*.*.*.{}", run_id);
        let consumer = /* create ephemeral pull consumer with filter */;
        let mut messages = consumer.messages().await.unwrap();
        let idle_dur = Duration::from_secs(config.idle_timeout_secs);

        loop {
            tokio::select! {
                _ = shared.shutdown.cancelled() => {
                    tracing::info!(%run_id, "Stream shutdown requested");
                    break;
                }
                msg = tokio::time::timeout(idle_dur, messages.next()) => {
                    match msg {
                        Ok(Some(Ok(nats_msg))) => {
                            let _ = nats_msg.ack().await;
                            // Zero-copy: broadcast the Bytes directly
                            let _ = shared.sender.send(nats_msg.payload.clone());
                        }
                        Ok(Some(Err(e))) => {
                            tracing::error!(%run_id, "NATS error: {}", e);
                        }
                        Ok(None) => break, // stream ended
                        Err(_) => {
                            // Idle timeout — no messages for N seconds
                            tracing::info!(%run_id, "Idle timeout, closing stream");
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup: remove from map (prevents new joins)
        shared.streams_map.remove(&run_id);
        // Channel is dropped when `shared` goes out of scope
    });
}
```

**Important**: The background task holds one `Arc<SharedStream>`. This means the `SharedStream` is only dropped when *both* the background task exits *and* all client `Arc`s are dropped. The idle timeout path removes the entry from the map first (preventing new clients from joining), then exits (dropping its `Arc`).

#### 3. Client Disconnection (Automatic Cleanup via `Drop`)

```rust
impl Drop for SharedStream {
    fn drop(&mut self) {
        // Cancel the background NATS pump task
        self.shutdown.cancel();
        // Remove from map (idempotent — may already be removed by idle timeout)
        self.streams_map.remove(&self.run_id);
    }
}
```

**No deadlock risk**: `DashMap::remove()` acquires the shard lock for the duration of the remove call only. It does not nest inside any other `DashMap` lock since `Drop` is called after the caller has released all `DashMap` references.

#### 4. External Trigger (Stop Stream API)

`DELETE /streams/{run_id}`:
1. Remove entry from `DashMap`.
2. If the `Weak` was upgradeable, call `shutdown.cancel()` on the `Arc<SharedStream>`.
3. This causes the background pump to break out → channel is dropped → all SSE receivers get `RecvError::Closed` → connections terminate.

### SSE Controller Integration

```rust
pub async fn stream_logs(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let shared = state.stream_manager.get_or_create_stream(&run_id).await;
    let mut rx = shared.sender.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    yield Ok::<_, Infallible>(
                        Event::default().data(text.as_ref())
                    );
                }
                Err(RecvError::Lagged(n)) => {
                    yield Ok(Event::default()
                        .event("warning")
                        .data(format!("Skipped {} messages due to slow consumption", n)));
                }
                Err(RecvError::Closed) => break,
            }
        }
        // `shared` (Arc<SharedStream>) dropped here when stream ends
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
```

### Future: Configurable Replay Modes

Per `architecture-points.md`, clients may request:
- **Last N lines**: `DeliverPolicy::Last` with `num_replicas` (not natively supported — requires `DeliverPolicy::All` with client-side tail, or `DeliverPolicy::LastPerSubject`).
- **Last 5 minutes**: `DeliverPolicy::ByStartTime { start_time }`.
- **From beginning**: `DeliverPolicy::All`.

> [!NOTE]
> When a client requests a replay mode other than "live tail", they get their **own** ephemeral consumer (no sharing) since the replay position is client-specific. The `StreamManager` handles this by **not** inserting into the shared map for replay-mode requests.
