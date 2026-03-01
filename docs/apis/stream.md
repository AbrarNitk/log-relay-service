# Stream APIs


## Start Stream

`GET /stream-logs/{run_id}`

This API is used to stream logs for a specific `run_id`. It supports different delivery and replay policies via query parameters.

### Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `delivery_policy` | string | `last` | Determines where the stream should begin. Options: `all`, `last`, `new`, `by_start_seq`, `by_start_time`, `last_per_subject`. |
| `replay_policy` | string | `original` | Determines the speed at which logs are delivered. Options: `instant`, `original`. |
| `start_seq` | number | - | **Required** if `delivery_policy=by_start_seq`. The sequence number to start streaming from. |
| `start_time` | string | - | **Required** if `delivery_policy=by_start_time`. An RFC3339 formatted date-time string to start streaming from. |


### Use Cases

#### 1. Stream newly arriving logs
Get only the newly arriving logs for a run after the connection is established. This is suitable for a real-time live-view.
```http
GET /stream-logs/my-run-123?delivery_policy=new
```

#### 2. Stream all logs from the beginning
Get the entire log history for a run, replaying them at their original speed.
```http
GET /stream-logs/my-run-123?delivery_policy=all
```

If you want to receive the existing logs as fast as possible instead of at their original timestamp intervals:
```http
GET /stream-logs/my-run-123?delivery_policy=all&replay_policy=instant
```

#### 3. Resume streaming from a specific sequence
If a stream was disconnected, you can resume exactly where you left off by providing the following sequence number.
```http
GET /stream-logs/my-run-123?delivery_policy=by_start_seq&start_seq=42
```

#### 4. Stream logs from a specific moment in time
Fetch logs starting from a specific timestamp.
```http
GET /stream-logs/my-run-123?delivery_policy=by_start_time&start_time=2024-03-01T10:00:00Z
```

#### 5. Get the last log message (Default)
The default behavior is to get the last message on the stream, and then listen for any new messages.
```http
GET /stream-logs/my-run-123
```
*(This is equivalent to `?delivery_policy=last`)*

#### 6. Get the last log message per subject
If logs are further divided by subject, you can fetch the most recent message for each matching subject, and then listen for new logs.
```http
GET /stream-logs/my-run-123?delivery_policy=last_per_subject
```
