# System Design: Log Relay Service

## 1. Overview

The Log Relay Service is a high-performance backend built in Rust. It serves as a dual-path log handling bridge between NATS JetStream and external consumers. 

It handles two primary jobs:
- **Real-time Streaming**: Delivering logs to web browsers in near real-time via Server-Sent Events (SSE).
- **Background Archiving**: Continuously polling logs and batch-archiving them to OpenObserve for long-term storage.

---

## 2. Core Operational Requirements

### 2.1. Log Producers (Applications)
Applications are dynamic and transient, running on orchestration platforms (e.g., Kubernetes).
- **Uniqueness**: Each application run must be identified by a unique `run-id`.
- **Log Routing**: Logs are sent to NATS JetStream using the subject pattern: `logs.<org>.<service>.<category>.<run_id>`.
- **Lifecycle Events**: Applications emit `start` and `end` events to signal log stream boundaries, but the relay service must also handle cases where these events are missed.

### 2.2. Message Broker (NATS JetStream)
NATS acts as the persistent, decoupled central log buffer.
- **Retention**: Must retain logs for a minimum of 24 hours.
- **Taxonomy**: Expanding subject taxonomy allows granular subscriptions (by organization, category, or specific run).
- **Decoupling**: Ensures that producers (applications) are completely decoupled from consumers (web clients and archivers).

### 2.3. Real-time Relay Server (SSE Streaming)
The API layer streaming logs to Web Clients. 

**Client Protocol & Reusability:**
- **SSE Connections**: Delivers logs to browsers over HTTP via reusable Server-Sent Events connections.
- **Connection Multiplexing**: If multiple clients request the same `run_id`, they must share a single underlying NATS ephemeral consumer. This prevents redundant network load and controls memory usage.

**Backpressure & Memory Protection:**
- **Bounded Channels**: Use fixed-size internal channels (e.g., Tokio `broadcast`) to protect internal memory from slow consumers.
- **Lag Handling**: Lagging clients should receive warning events and potentially be disconnected gracefully rather than degrading the entire service.
- **Zero-Copy Delivery**: Where possible, pass raw `Bytes` direct from NATS to the HTTP serialization boundary without allocating new strings in memory.

**Reliability & Reconnection:**
- **Automatic Resume**: Use JetStream sequence numbers as `Last-Event-ID`. Reconnecting clients start exactly where they left off, avoiding duplicates or missed logs.
- **Historical Querying**: Clients can specify starting points (e.g., `since=5m`, `ts=<timestamp>`).
- **Pacing**: Paced replay of historical logs to avoid overwhelming the browser's JavaScript engine.

**Lifecycle & Cleanup:**
- **Controlled Disconnects**: Close the SSE stream automatically if an explicit "termination" event is received from the application.
- **Idle Timeout**: Drop inactive streams (e.g., no logs received for 5 minutes).
- **Resource Cleanup**: When the last active SSE client for a specific `run_id` disconnects, immediately clean up the underlying NATS consumer.

### 2.4. Background Archiver
The background worker fleet responsible for long-term audit and storage.

**Processing Flow:**
- **Pattern Matching**: Consumes logs from a wildcard subject (`logs.>`).
- **Data Enrichment**: Parsed NATS subject metadata (`org`, `service`, `category`, `run_id`) is injected into the JSON payload before storage.
- **Batching**: Reduces API calls to OpenObserve by batching messages based on count size and timeout limits.

**Reliability Mechanisms:**
- **Explicit ACKs**: Requires `AckPolicy::Explicit`. Only acknowledges NATS once OpenObserve replies with an HTTP 200 OK.
- **Retry Logic**: Re-attempts OpenObserve dispatches utilizing exponential backoff on transient errors. NACKs messages backed by NATS delivery retries when thresholds are crossed.

---

## 3. Scalability, Concurrency, and Infrastructure

Given the choice of **Rust** and **Tokio** runtime, the system should guarantee minimum CPU/RAM overhead per active connection.

**Horizontal Scalability:**
- **Relay Server Fleet**: Servers are stateless in regards to individual clients. Load balancers can route clients to any node. JetStream supports separate ephemeral consumers for different instances.
- **Archiver Fleet**: Uses JetStream Consumer Groups to automatically shard the workload across multiple archiver instances, increasing write-throughput to OpenObserve.

**Observability & Monitoring:**
- **Exposed Metrics**: The system must track and expose active streams, connected clients, OpenObserve batch sizes, API latency, and lag (NATS sequence drift). 
- **Tracing**: Structured, leveled logging (e.g., `tracing` crate) to debug connection lifecycles in real-time.

**Security & Throttling:**
- **Authentication**: SSE endpoints should be protected (using mechanisms like JWTs or secure tokens) to prevent unauthorized arbitrary log access.
- **Rate Limiting**: Prevent individual IPs from opening too many concurrent SSE streams.
- **CORS**: Correct Cross-Origin Resource Sharing headers for browser integrations.

---

## 4. Key Considerations Missed in Original Drafts

1. **Paced Historical Log Replay**: Browsers easily crash if fed 10,000 logs instantly upon connection. Sending historical logs requires backpressure toward NATS or timed chunking.
2. **CORS & Authentication Flow**: SSE connections don't natively support setting standard `Authorization: Bearer <token>` headers easily in all browsers without query params or cookies. The requirement should dictate using a short-lived token generated via another secure API endpoint for the SSE connection parameter.
3. **Consumer Groups for Archivers**: Explicitly defining that the background archivers should run in a JetStream "Consumer Group" / Shared Durable queue. This enables high availability and load distribution for archiving.
4. **Rate Limiting (DDoS Protection)**: The SSE endpoint is inherently vulnerable to connection exhaustion. Proper connection limits per origin/IP are required at the app or proxy level.
