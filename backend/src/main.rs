use vision_event_api::{
    api,
    application::AppState,
    persistence::{Database, DatabaseConfig},
    progress::RedisProgressStore,
    queue::TaskQueue,
};

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let database = match std::env::var("DATABASE_URL") {
        Ok(url) => match Database::connect(&DatabaseConfig::new(url)).await {
            Ok(database) => {
                if let Err(error) = database.migrate().await {
                    eprintln!("PostgreSQL migration failed: {error}");
                }
                println!("connected to PostgreSQL for jobs and events");
                Some(database)
            }
            Err(error) => {
                eprintln!("PostgreSQL unavailable, using memory fallback: {error}");
                None
            }
        },
        Err(_) => {
            eprintln!("DATABASE_URL is missing; API is using in-memory storage only");
            None
        }
    };
    let redis_url = std::env::var("REDIS_URL").ok();
    let queue = redis_url
        .as_deref()
        .and_then(|url| TaskQueue::new(url, "vision:jobs").ok());
    let progress_store = redis_url
        .as_deref()
        .and_then(|url| RedisProgressStore::new(url, "vision:job-progress").ok());
    let state = AppState::default()
        .with_integrations(database.clone(), queue.clone())
        .with_progress_store(progress_store);
    if let Some(database) = &database {
        match database.list_rules().await {
            Ok(rules) => {
                for rule in rules {
                    state.update_rule(rule.event_type.clone(), rule);
                }
            }
            Err(error) => eprintln!("failed to load event rules from PostgreSQL: {error}"),
        }
    }
    if std::env::var("WORKER_MODE").ok().as_deref() == Some("1") {
        if let Some(queue) = queue {
            vision_event_api::worker::run_loop(state, queue).await;
        } else {
            eprintln!("WORKER_MODE=1 but REDIS_URL is missing");
        }
        return;
    }
    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("bind API port");
    println!(
        "vision-event-api listening on {}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app).await.expect("serve API");
}
