pub mod blocking_in_async;
pub mod lock_across_await;
pub mod unbounded_spawn;

use crate::{analysis::Finding, github::models::ChangedFile};

pub trait Detector: Send + Sync {
    fn id(&self) -> &'static str;
    fn analyze_patch(&self, file: &ChangedFile) -> Vec<Finding>;
}

pub fn all() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(lock_across_await::LockAcrossAwait),
        Box::new(blocking_in_async::BlockingInAsync),
        Box::new(unbounded_spawn::UnboundedSpawn),
    ]
}
