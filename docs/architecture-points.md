## Requirements and Note Points

- Single Nats TCP Connection to N Virtual SSE Connections
- Share same consumer with multiple sse events with the same run-id
- if the same web consumer reconnects with the same run-id, it uses the same consumer from nats
- in the web-client, perhaps people can ask stream the logs last 5 minutes or last 10 minutes
  or 100 lines or from the beginning.
- log stream termination based on the time, or based on the event from other activity
- log stream based on the time, like 10 minutes or 1 hour or 1 day
- connection must terminated gracefully, and cleanup all the resources
- after terminating the sse event, we should also notify the browser as well for graceful termination
  - events can be type of connected
  - event can be of log stream messages
  - event can be of error messages
  - event can be of termination