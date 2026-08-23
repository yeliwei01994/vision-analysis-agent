use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    pub job_id: String,
    pub attempt: u32,
}
impl QueueMessage {
    pub fn new(job_id: Uuid) -> Self {
        Self {
            job_id: job_id.to_string(),
            attempt: 0,
        }
    }
}

#[derive(Clone)]
pub struct TaskQueue {
    client: redis::Client,
    stream: String,
    last_id: Arc<Mutex<String>>,
}

impl TaskQueue {
    pub fn new(url: &str, stream: impl Into<String>) -> Result<Self, redis::RedisError> {
        Ok(Self {
            client: redis::Client::open(url)?,
            stream: stream.into(),
            last_id: Arc::new(Mutex::new("0-0".into())),
        })
    }
    pub async fn enqueue(&self, message: &QueueMessage) -> redis::RedisResult<String> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let payload = serde_json::to_string(message).unwrap_or_default();
        redis::cmd("XADD")
            .arg(&self.stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut connection)
            .await
    }

    pub async fn consume_once(&self) -> redis::RedisResult<Option<QueueMessage>> {
        let mut connection = self.client.get_multiplexed_async_connection().await?;
        let last_id = self.last_id.lock().expect("queue cursor poisoned").clone();
        let reply: redis::streams::StreamReadReply = redis::cmd("XREAD")
            .arg("BLOCK")
            .arg(1_000)
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(&self.stream)
            .arg(last_id)
            .query_async(&mut connection)
            .await?;
        let payload = reply
            .keys
            .into_iter()
            .flat_map(|key| key.ids)
            .inspect(|entry| {
                *self.last_id.lock().expect("queue cursor poisoned") = entry.id.clone();
            })
            .find_map(|entry| entry.map.get("payload").cloned());
        match payload {
            Some(value) => {
                let text: String = redis::from_redis_value(&value)?;
                serde_json::from_str(&text).map(Some).map_err(|_| {
                    redis::RedisError::from((redis::ErrorKind::TypeError, "invalid queue payload"))
                })
            }
            None => Ok(None),
        }
    }
}
