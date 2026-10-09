//! T1Bridge renderer client, vendored from the touchbar-doom example in
//! https://github.com/standardagents/t1bridge (v0.1.12). MIT licensed: see
//! ../../LICENSES/touchbar-client-MIT.txt. Local change: T1_DASH_TEST_SOCKET
//! in transport.rs points the client at a fake service for tests.

pub mod client;
pub mod transport;
pub mod wire;

pub use client::{ActionKind, Brightness, Capabilities, Client, ClientError, Event};
pub use wire::{Contact, Damage, Dimensions, InputFrame, Key, ServiceError};
