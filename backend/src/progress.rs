use crate::domain::{unix_time_millis, JobProgressEvent};
use redis::streams::{StreamRangeReply, StreamReadReply};
use redis::Script;

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
        event: JobProgressEvent,
    ) -> redis::RedisResult<Option<StoredJobProgress>> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let job_id = event.job_id.to_string();
        let updated_at = unix_time_millis();
        let payload = serialize_event(&event)?;
        let (accepted, stream_id, stored_payload): (i64, String, String) = Script::new(r#"
            local existing = redis.call('HGET', KEYS[1], ARGV[1])
            if existing then
                local current = cjson.decode(existing)
                local incoming = cjson.decode(ARGV[3])
                local current_attempt = tonumber(current.attempt or 0)
                local incoming_attempt = tonumber(incoming.attempt or 0)
                local terminal = current.status == 'completed' or current.status == 'failed' or current.status == 'cancelled'
                if incoming_attempt < current_attempt or (incoming_attempt == current_attempt and terminal) then
                    return { 0, '', existing }
                end
            end
            local sequence = redis.call('HINCRBY', KEYS[2], ARGV[1], 1)
            local incoming = cjson.decode(ARGV[3])
            incoming.sequence = sequence
            incoming.updated_at = tonumber(ARGV[2])
            local stored = cjson.encode(incoming)
            local stream_id = redis.call('XADD', KEYS[3], 'MAXLEN', '~', ARGV[4], '*', 'payload', stored)
            redis.call('HSET', KEYS[1], ARGV[1], stored)
            return { 1, stream_id, stored }
        "#)
            .key(&self.snapshots)
            .key(&self.sequences)
            .key(&self.stream)
            .arg(&job_id)
            .arg(updated_at)
            .arg(payload)
            .arg(MAX_PROGRESS_EVENTS)
            .invoke_async(&mut connection)
            .await?;
        if accepted == 0 {
            return Ok(None);
        }
        Ok(Some(StoredJobProgress {
            stream_id,
            event: deserialize_event(&stored_payload)?,
        }))
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

    pub async fn trim_to(&self, max_len: usize) -> redis::RedisResult<i64> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        redis::cmd("XTRIM").arg(&self.stream).arg("MAXLEN").arg("=").arg(max_len).query_async(&mut connection).await
    }

    pub async fn cursor_is_trimmed(&self, cursor: &str) -> redis::RedisResult<bool> {
        if cursor == "0-0" { return Ok(false); }
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let reply: StreamRangeReply = redis::cmd("XRANGE").arg(&self.stream).arg("-").arg("+").arg("COUNT").arg(1).query_async(&mut connection).await?;
        Ok(reply.ids.first().is_some_and(|entry| stream_id_less(cursor, &entry.id)))
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

fn stream_id_less(left: &str, right: &str) -> bool {
    let parse = |value: &str| value.split_once('-').and_then(|(ms, seq)| Some((ms.parse::<u64>().ok()?, seq.parse::<u64>().ok()?)));
    match (parse(left), parse(right)) { (Some(left), Some(right)) => left < right, _ => false }
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
