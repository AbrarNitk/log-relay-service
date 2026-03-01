# System Design

We have to stream the application logs to browser so that users can see them in near realtime.


## Requirements

- applications
  - there are going N number of apps coming and going and generating logs in nats
  - apps also sent start and end stream logs, but sometimes they missed it this events.
  - each app have a unique run-id attached to it
- nats-jet-stream
  - we save the logs in nats jet-stream for minimum of 24hrs
  - the subject-name will be dynamic based on the run-id
  - the subject will have different components to it and we will enrich the event
    data based on that.
  - subject name follow the pattern: `logs.<org>.<service>.<category>.<execution_id>`
- relay-server
  - relay server service the logs to the browser as sse events
  - it takes the run-id and params like since when we stream the events
  - it needs to handle the backpressure also, meaning events should be delivered to browser as they are being generated.
  - consumers can  be reusable based on the timing parameters to the sse connections.
  - it should be able to terminate the connection based on the app terminate event which
    are being send in other topic or based on the time.
  - once the sse connection closed dynamically running consumer must be end and cleanup the resource as well.
  - this service must be highly scaleable from resource consuming perpective
  - browser may disconnects and re-connects again(network issues), it should not lost the events
  - it should eat unnecessary CPU
  - it should eat unecessary RAM
  - this must have a different kinds of events types
    - connected
    - log
    - error
    - disconnected
    - stream-ended
    - heart-beat event
    - keep-alive connection
      - we need to figure out, keep alive should be property of the http connection, like sending the heartbeat
      - we also need to check that if it is possible for us to use the http/2 protocol
      - and what are the benefit pros and cons exist
  - automatic reconnect support
    - each event log includes an id, which is a jetstream sequence number
    - when reconnecting browser send the `last-event-id` in the header or query, if nothing then starts from now.
    - logs automatically resumes from that event number or least nearest to that(so no duplicates or missed up messages)
  - request may include the parameter
    - since=instant,from-start,duration(5m,1h,30s,2d, etc) and ts=<datetime timestamp>, and event-id
    - in that case we should bombard the browser, we can implement a mechanism
- background-archiever: long running consumer
  - continuously pulls the messaage in N number of chunks
  - parses the subjects
  - enriches the logs
  - sends to openobserve
  - Retries on failure
  - NACKs on failure after N retries
  - it will run in the separate thread
  - it will keep consuming events from all the application and stream them to openobserve
  - this service must be highly scaleable from resource consuming perpective
  - this consumer will also enrich the events
