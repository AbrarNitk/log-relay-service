## Requirements and Note Points: V1

- Single Nats TCP Connection to N Virtual SSE Connections
- in the web-client, perhaps people can ask stream the logs last 5 minutes or last 10 minutes
  or 100 lines or from the beginning.
- log stream termination based on the time, or based on the event from other activity, like close the connection
  - connection must terminated gracefully, and cleanup all the resources
  - after terminating the sse event, we should also notify the browser as well for graceful termination
  - events can be type of connected
  - event can be of log stream messages
  - event can be of error messages
  - event can be of termination
- log-relay server can also keep sending the hert-beat event to the browser to tell that stream is not yet ended.
- how to control the flow of the logs as the application generates the logs


## Questions

- by default where should we stream the logs

## Deferred Or V2

- Share same consumer with multiple sse events with the same run-id
  - if the same web consumer reconnects with the same run-id, it uses the same consumer from nats
- nats even can send the messages and plain binary and in the browser we can parse thee message, using nats.ws library
- we can have redis working in a separate which will tell us about that the run has beeen started or ended, or terminated
