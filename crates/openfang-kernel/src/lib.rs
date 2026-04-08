//! Core kernel for the OpenFang Agent Operating System.
//!
//! The kernel manages agent lifecycles, memory, permissions, scheduling,
//! and inter-agent communication.

pub mod approval;
pub mod auth;
pub mod auto_reply;
pub mod background;
pub mod backlog_store;
pub mod backlog_watcher;
pub mod capabilities;
pub mod config;
pub mod config_reload;
pub mod cron;
pub mod error;
pub mod event_bus;
pub mod git_worktree;
pub mod heartbeat;
pub mod kernel;
pub mod metering;
pub mod pairing;
pub mod project_context;
pub mod project_store;
pub mod registry;
pub mod scheduler;
pub mod supervisor;
pub mod triggers;
pub mod wizard;
pub mod workflow;

pub use backlog_store::BacklogStore;
pub use backlog_watcher::BacklogWatcherManager;
pub use kernel::DeliveryTracker;
pub use kernel::OpenFangKernel;
