use std::time::Duration;
use crate::runtime::task_registry::{ShutdownReport, TaskRegistry};

pub const DEFAULT_JOIN_DEADLINE: Duration = Duration::from_millis(500);

pub fn shutdown(registry: &TaskRegistry) -> ShutdownReport { registry.shutdown(DEFAULT_JOIN_DEADLINE) }
