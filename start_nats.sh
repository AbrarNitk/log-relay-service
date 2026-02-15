#!/bin/bash
set -e

# Configuration
NATS_SERVER="nats://localhost:4222"
NATS_USER="nats"
NATS_PASS="nats"

# Stream configuration
STREAM_NAME="LOGS"
SUBJECTS=("logs.job.>" "logs.job_activity.>")

echo "Starting NATS Server..."
# Start NATS server in background with JetStream enabled
nats-server -js --user "$NATS_USER" --pass "$NATS_PASS" &
NATS_PID=$!

# Wait for NATS to be ready
echo "Waiting for NATS to be ready..."
until nats server check connection --server "$NATS_SERVER" --user "$NATS_USER" --password "$NATS_PASS" >/dev/null 2>&1; do
    sleep 1
done
echo "NATS is up!"

# Build --subjects flags
SUBJECT_ARGS=()
for subj in "${SUBJECTS[@]}"; do
    SUBJECT_ARGS+=("--subjects" "$subj")
done

# Create JetStream Stream using nats CLI
echo "Creating JetStream Stream: $STREAM_NAME"
nats stream add "$STREAM_NAME" \
    "${SUBJECT_ARGS[@]}" \
    --storage file \
    --retention limits \
    --discard old \
    --max-msgs-per-subject=-1 \
    --dupe-window 2m \
    --replicas 1 \
    --server "$NATS_SERVER" \
    --user "$NATS_USER" \
    --password "$NATS_PASS" \
    --defaults

echo "Stream $STREAM_NAME created successfully with subjects: ${SUBJECTS[*]}"

# Keep script running to keep server alive, or exit if server stops
wait $NATS_PID
