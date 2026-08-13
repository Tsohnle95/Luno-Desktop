pub mod contracts;
pub mod persistence {
    pub mod atomic_write;
    pub mod recovery;
}
pub mod runtime {
    pub mod shutdown;
    pub mod task_registry;
}
pub mod state;
