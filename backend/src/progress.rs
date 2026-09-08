use crate::domain::{unix_time_millis, JobProgressEvent};
use redis::streams::{StreamRangeReply, StreamReadReply};

const MAX_PROGRESS_EVENTS: usize = 10_000;

#[derive(Debug, Clone)]
pub struct StoredJobProgress {
    pub stream_id: String,
    pub event: JobProgressEvent,
}

#[derive(Clone)]
pub struct RedisProgressStore {
    client: redis::Client,
    stream: String,
    snapshots: String,
    sequences: String,
}

impl RedisProgressStore {
    pub fn new(url: &str, stream: impl Into<String>) -> Result<Self, redis::RedisError> {
        let stream = stream.into();
        Ok(Self {
            client: redis::Client::open(url)?,
            snapshots: format!("{stream}:snapshots"),
            sequences: format!("{stream}:sequences"),
            stream,
        })
    }

    pub async fn publish(
        &self,
        mut event: JobProgressEvent,
    ) -> redis::RedisResult<StoredJobProgress> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let sequence: u64 = redis::cmd("HINCRBY")
            .arg(&self.sequences)
            .arg(event.job_id.to_string())
            .arg(1)
            .query_async(&mut connection)
            .await?;
        event.sequence = sequence;
        event.updated_at = unix_time_millis();
        let payload = serialize_event(&event)?;
        let (stream_id, _snapshot_written): (String, i64) = redis::pipe()
            .atomic()
            .cmd("XADD")
            .arg(&self.stream)
            .arg("MAXLEN")
            .arg("~")
            .arg(MAX_PROGRESS_EVENTS)
            .arg("*")
            .arg("payload")
            .arg(&payload)
            .cmd("HSET")
            .arg(&self.snapshots)
            .arg(event.job_id.to_string())
            .arg(payload)
            .query_async(&mut connection)
            .await?;
        Ok(StoredJobProgress {
            stream_id,
            event,
        })
    }

    pub async fn snapshots(&self) -> redis::RedisResult<Vec<JobProgressEvent>> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let payloads: Vec<String> = redis::cmd("HVALS")
            .arg(&self.snapshots)
            .query_async(&mut connection)
            .await?;
        let mut events = payloads
            .into_iter()
            .map(|payload| deserialize_event(&payload))
            .collect::<redis::RedisResult<Vec<_>>>()?;
        events.sort_by_key(|event| (event.updated_at, event.job_id));
        Ok(events)
    }

    pub async fn latest_stream_id(&self) -> redis::RedisResult<String> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let reply: StreamRangeReply = redis::cmd("XREVRANGE")
            .arg(&self.stream)
            .arg("+")
            .arg("-")
            .arg("COUNT")
            .arg(1)
            .query_async(&mut connection)
            .await?;
        Ok(reply
            .ids
            .first()
            .map(|entry| entry.id.clone())
            .unwrap_or_else(|| "0-0".into()))
    }

    pub async fn read_after(
        &self,
        cursor: &str,
        block_ms: usize,
    ) -> redis::RedisResult<Vec<StoredJobProgress>> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let reply: StreamReadReply = redis::cmd("XREAD")
            .arg("BLOCK")
            .arg(block_ms)
            .arg("COUNT")
            .arg(64)
            .arg("STREAMS")
            .arg(&self.stream)
            .arg(cursor)
            .query_async(&mut connection)
            .await?;
        let mut events = Vec::new();
        for entry in reply.keys.into_iter().flat_map(|key| key.ids) {
            let Some(value) = entry.map.get("payload") else {
                continue;
            };
            let payload: String = redis::from_redis_value(value)?;
            events.push(StoredJobProgress {
                stream_id: entry.id,
                event: deserialize_event(&payload)?,
            });
        }
        Ok(events)
    }

    pub async fn remove_job(&self, job_id: uuid::Uuid) -> redis::RedisResult<()> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        redis::pipe()
            .atomic()
            .cmd("HDEL")
            .arg(&self.snapshots)
            .arg(job_id.to_string())
            .ignore()
            .cmd("HDEL")
            .arg(&self.sequences)
            .arg(job_id.to_string())
            .ignore()
            .query_async(&mut connection)
            .await
    }

    pub async fn clear(&self) -> redis::RedisResult<()> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        redis::cmd("DEL")
            .arg(&self.stream)
            .arg(&self.snapshots)
            .arg(&self.sequences)
            .query_async::<()>(&mut connection)
            .await
    }
}

fn serialize_event(event: &JobProgressEvent) -> redis::RedisResult<String> {
    serde_json::to_string(event).map_err(|error| {
        redis::RedisError::from((
            redis::ErrorKind::TypeError,
            "job progress serialization failed",
            error.to_string(),
        ))
    })
}

fn deserialize_event(payload: &str) -> redis::RedisResult<JobProgressEvent> {
    serde_json::from_str(payload).map_err(|error| {
        redis::RedisError::from((
            redis::ErrorKind::TypeError,
            "invalid job progress payload",
            error.to_string(),
        ))
    })
}
