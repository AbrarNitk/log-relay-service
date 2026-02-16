# Log Relay Service


## Application

We are going to have dynamic runtime applications that will be deployed on k8s cluster.
These applications will generate logs and we collect them in nats jetstream.

Each application will unqiue run-id and we will use this to identify the logs of that application.


## Log Relay Service

A high-performance, lightweight Rust backend bridge with a dual-path log handling 
system: background archiving to OpenObserve and real-time streaming to web clients 
via Server-Sent Events (SSE).


## Requirements: Log Relay Service

### Archive Logs to OpenObserve

A consumer polls the logs from nats and archives them to openobserve.

This must be an independent consumer which read the logs from nats subject 
logs.* and bulk logs to the openobserve.


The subject is going to be in the form of `logs.<a>.<b>.<c>.<d>`, it parses the a,b,c,d
and batches logs to the openobserve. Also it enriches the logs with a,b,c,d.

On successful batch, it sends a acknowledgement to nats for all the messages.


### Real-time Logs Streaming

We have to stream the logs to the web clients via Server-Sent Events (SSE)
as path `/stream-logs/:run-id`.

When a request comes for the `run-id`, we start filtering logs from nats,
and start sending them to the client.

We can filter the subjects `logs.*.*.*.{run-id}` and do not need to filter at the application level.

We can end the stream and termainate the sse connection after 5 minutes if there are no logs are streaming.

The application can send a signal to the server to start or stop the stream and terminate the SSE connection.
When the client disconnects, the server must automatically delete the consumer and clean up all associated resources.

## Architecture & Implementation Details

### Configuration
The service requires the following configuration (via file or environment variables):
- **NATS**: URL, Credentials.
- **OpenObserve**:
  - `OPENOBSERVE_URL`: Base URL for the OpenObserve instance.
  - `OPENOBSERVE_ORG`: Organization identifier.
  - `OPENOBSERVE_USER`: Username/Email.
  - `OPENOBSERVE_PASSWORD`: Password/Token.
  - `OPENOBSERVE_STREAM`: Target stream name (default: `default`).
- **Relay**:
  - `BATCH_SIZE`: Number of logs to batch before sending (e.g., 100).
  - `BATCH_TIMEOUT`: Max time to wait before sending a partial batch (e.g., 5s).

### Subject Hierarchy

The NATS subject hierarchy `logs.<a>.<b>.<c>.<d>` maps to:
`logs.<namespace>.<application>.<component>.<run_id>`
- Wildcard subscription for archiving: `logs.>`
- Filtered subscription for streaming: `logs.*.*.*.<run_id>`

### Reliability & Error Handling
- **Archiving**:
  - Use NATS JetStream with `AckPolicy::Explicit`.
  - Acknowledge messages only after receiving a 200 OK from OpenObserve.
  - Implement exponential backoff for OpenObserve connection failures.
- **Streaming**:
  - Use ephemeral, ordered JetStream consumers for live tailing to ensure correct order.
  - Heartbeat/Keep-alive for SSE connections to detect dead clients.

### Security
- **SSE Endpoint**: Should be protected (e.g., via shared secret or JWT) to prevent unauthorized stream access.



```
# 1. Start NATS
bash start_nats.sh
# 2. Start server
cargo run -p logs-server
# 3. Start generator
cargo run -p generator -- --run-id test123 --namespace dev --application myapp --component worker --pressure 5
# 4. Connect SSE client
curl -N http://localhost:3000/stream-logs/test123
# 5. Stop stream (optional)
curl -X DELETE http://localhost:3000/streams/test123

```


```
# 1. Compile
cargo build --workspace
# 2. Start NATS + generator
bash start_nats.sh
cargo run -p generator -- --run-id test123 -n dev -a myapp --component worker -p 10
# 3. Start archiver (with OpenObserve running)
cargo run -p archiver
# 4. Query OpenObserve to verify logs arrived
curl -u "root@example.com:Complexpass#123" \
  "http://localhost:5080/api/default/_search" \
  -d '{"query":{"sql":"SELECT * FROM default ORDER BY _timestamp DESC LIMIT 10"}}'
```


```
docker run -d \
  --name openobserve \
  -p 5080:5080 \
  -e ZO_ROOT_USER_EMAIL="root@example.com" \
  -e ZO_ROOT_USER_PASSWORD="Complexpass#123" \
  public.ecr.aws/zinclabs/openobserve:latest
```