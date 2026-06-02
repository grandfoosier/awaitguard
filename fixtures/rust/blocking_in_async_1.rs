// Fixture: blocking call in async — should trigger blocking_in_async detector
use std::time::Duration;

async fn do_work() {
    // BUG: blocks the executor thread
    std::thread::sleep(Duration::from_secs(1));
    println!("done");
}
