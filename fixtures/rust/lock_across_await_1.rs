// Fixture: lock held across await — should trigger lock_across_await detector
use std::sync::Arc;
use tokio::sync::Mutex;

async fn handle(state: Arc<Mutex<Vec<String>>>, event: String) {
    let mut guard = state.lock().await;
    // BUG: guard is still held while we await an I/O call
    let processed = call_remote(&event).await;
    guard.push(processed);
}

async fn call_remote(event: &str) -> String {
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    format!("ok:{event}")
}
