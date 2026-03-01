use async_nats::jetstream::consumer::{DeliverPolicy, ReplayPolicy};
use axum::http::StatusCode;
use std::convert::TryInto;

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryDeliverPolicy {
    All,
    #[default]
    Last,
    New,
    ByStartSeq,
    ByStartTime,
    LastPerSubject,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryReplayPolicy {
    Instant,
    #[default]
    Original,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct StreamCreateQuery {
    #[serde(default)]
    pub delivery_policy: QueryDeliverPolicy,

    #[serde(default)]
    pub replay_policy: QueryReplayPolicy,

    // Configured if delivery_policy is ByStartSequence
    #[serde(default)]
    pub start_seq: Option<u64>,

    // Configured if delivery_policy is ByStartTime, RFC3339 string
    #[serde(default)]
    pub start_time: Option<String>,
}

pub struct StreamPolicies {
    pub deliver_policy: DeliverPolicy,
    pub replay_policy: ReplayPolicy,
}

impl TryInto<StreamPolicies> for StreamCreateQuery {
    type Error = (StatusCode, &'static str);

    fn try_into(self) -> Result<StreamPolicies, Self::Error> {
        let deliver_policy = match self.delivery_policy {
            QueryDeliverPolicy::All => DeliverPolicy::All,
            QueryDeliverPolicy::Last => DeliverPolicy::Last,
            QueryDeliverPolicy::New => DeliverPolicy::New,
            QueryDeliverPolicy::ByStartSeq => {
                let seq = self.start_seq.ok_or((
                    StatusCode::BAD_REQUEST,
                    "start_seq must be provided when delivery_policy is by_start_seq",
                ))?;
                DeliverPolicy::ByStartSequence {
                    start_sequence: seq,
                }
            }
            QueryDeliverPolicy::ByStartTime => {
                let time_str = self.start_time.ok_or((
                    StatusCode::BAD_REQUEST,
                    "start_time must be provided when delivery_policy is by_start_time",
                ))?;
                let format = time::format_description::well_known::Rfc3339;
                let parsed_time =
                    time::OffsetDateTime::parse(&time_str, &format).map_err(|_| {
                        (
                            StatusCode::BAD_REQUEST,
                            "start_time must be a valid RFC3339 date-time string",
                        )
                    })?;
                DeliverPolicy::ByStartTime {
                    start_time: parsed_time,
                }
            }
            QueryDeliverPolicy::LastPerSubject => DeliverPolicy::LastPerSubject,
        };

        let replay_policy = match self.replay_policy {
            QueryReplayPolicy::Instant => ReplayPolicy::Instant,
            QueryReplayPolicy::Original => ReplayPolicy::Original,
        };

        Ok(StreamPolicies {
            deliver_policy,
            replay_policy,
        })
    }
}
