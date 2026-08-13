use std::sync::Arc;
use std::time::Duration;
use crate::contracts::types::{Generation, SessionId};
use crate::runtime::task_registry::{CancellationToken, ShutdownReport, SpawnError, TaskHandle, TaskRegistry, TaskSpec};

pub struct AppState { pub tasks: Arc<TaskRegistry> }

impl AppState {
    pub fn new(task_limit: usize) -> Self { Self { tasks: Arc::new(TaskRegistry::new(task_limit)) } }
    pub fn submit<T, F>(&self, spec: TaskSpec, work: F) -> Result<TaskHandle<T>, SpawnError>
    where T: Send + 'static, F: FnOnce(CancellationToken) -> T + Send + 'static { self.tasks.spawn(spec, work) }
    pub fn generation(&self, key: impl Into<String>, generation: Generation) -> TaskSpec { TaskSpec::new(key, generation, None) }
    pub fn session_task(&self, key: impl Into<String>, generation: Generation, session: SessionId) -> TaskSpec { TaskSpec::new(key, generation, Some(session)) }
    pub fn publish_allowed(&self, spec: &TaskSpec) -> bool { self.tasks.is_current(spec) }
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport { self.tasks.shutdown(deadline) }
    pub fn is_closing(&self) -> bool { self.tasks.is_closing() }
}
