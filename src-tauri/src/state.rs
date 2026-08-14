use std::sync::Arc;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use crate::contracts::types::{Generation, SessionId};
use crate::runtime::task_registry::{CancellationToken, ShutdownReport, SpawnError, TaskHandle, TaskRegistry, TaskSpec};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus { NotRun, Completed { warnings: usize, snapshot_created: bool, removed_previews: usize }, Failed { reason: String } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_status_is_sanitized_diagnostic_data() {
        let status = MigrationStatus::Completed { warnings: 1, snapshot_created: true, removed_previews: 2 };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("snapshot_created"));
        assert!(!json.contains("/"));
    }
}

pub struct AppState { pub tasks: Arc<TaskRegistry>, pub migration_status: Arc<Mutex<MigrationStatus>> }

impl AppState {
    pub fn new(task_limit: usize) -> Self { Self { tasks: Arc::new(TaskRegistry::new(task_limit)), migration_status: Arc::new(Mutex::new(MigrationStatus::NotRun)) } }
    pub fn submit<T, F>(&self, spec: TaskSpec, work: F) -> Result<TaskHandle<T>, SpawnError>
    where T: Send + 'static, F: FnOnce(CancellationToken) -> T + Send + 'static { self.tasks.spawn(spec, work) }
    pub fn generation(&self, key: impl Into<String>, generation: Generation) -> TaskSpec { TaskSpec::new(key, generation, None) }
    pub fn session_task(&self, key: impl Into<String>, generation: Generation, session: SessionId) -> TaskSpec { TaskSpec::new(key, generation, Some(session)) }
    pub fn publish_allowed(&self, spec: &TaskSpec) -> bool { self.tasks.is_current(spec) }
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport { self.tasks.shutdown(deadline) }
    pub fn is_closing(&self) -> bool { self.tasks.is_closing() }
}
