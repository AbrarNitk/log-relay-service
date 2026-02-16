## Note Points

- N Consumers to 1 TCP Connections
- Single Nats TCP Connection to N Virtual Connections
- if the same web consumer reconnects, it will use the same consumer from nats
- in the web-client, perhaps people can ask stream the logs last 5 minutes or last 
  100 lines or from the beginning.
