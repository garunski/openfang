//! Channel Bridge Layer for the OpenFang Agent OS.
//!
//! Provides pluggable messaging integrations that convert platform messages
//! into unified `ChannelMessage` events for the kernel.

pub mod bridge;
pub mod formatter;
pub mod mattermost;
pub mod router;
pub mod signal;
pub mod types;
