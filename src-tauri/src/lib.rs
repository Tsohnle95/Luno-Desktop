pub mod contracts;
pub mod domain {
    pub mod identity;
    pub mod normalization;
}
pub mod logging {
    pub mod redaction;
    pub mod service;
}
pub mod persistence {
    pub mod atomic_write;
    pub mod recovery;
    pub mod legacy;
}
pub mod migration {
    pub mod legacy_readers;
}
pub mod runtime {
    pub mod shutdown;
    pub mod task_registry;
}
pub mod process {
    pub mod executables;
    pub mod supervisor;
}
pub mod download {
    pub mod promotion;
}
pub mod state;
pub mod library {
    pub mod index;
    pub mod reconcile;
    pub mod scanner;
    pub mod watch;
}
pub mod transfer {
    pub mod codec;
    pub mod planner;
    pub mod provider_index;
}
pub mod playback {
    pub mod backend;
    pub mod backend_adapter;
}
