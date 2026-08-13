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
    pub mod first_run;
    pub mod keychain;
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
    pub mod csv;
    pub mod coordinator;
    pub mod markers;
    pub mod promotion;
    pub mod sync;
    pub mod workflows;
}
pub mod state;
pub mod library {
    pub mod favorites;
    pub mod duplicates;
    pub mod index;
    pub mod mutations;
    pub mod reconcile;
    pub mod reference_repair;
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
    pub mod crossfade;
    pub mod clock;
    pub mod queue;
    pub mod recovery;
    pub mod sleep_timer;
}
pub mod discovery {
    pub mod service;
    pub mod artwork;
}
pub mod preview {
    pub mod assets;
    pub mod discover_mode;
    pub mod save;
    pub mod session;
}
pub mod platform { pub mod media_session; }
pub mod video {
    pub mod player;
    pub mod source;
    pub mod staging;
}
