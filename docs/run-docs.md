# Run Docs


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