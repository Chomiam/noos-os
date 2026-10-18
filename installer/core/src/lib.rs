//! noos-os installation engine.
//!
//! Deliberately free of any Tauri dependency: the Tauri app, a CLI and the
//! test suite all drive the same code. Anything that only *describes* the
//! install (partition sizes, `fstab`, `limine.conf`, snapper settings) is a
//! pure function, so it can be verified without root; only [`exec`] and
//! [`install`] touch the machine.

pub mod btrfs;
pub mod config;
pub mod desktop;
pub mod error;
pub mod events;
pub mod exec;
pub mod gpu;
pub mod hardware;
pub mod install;
pub mod kernel;
pub mod keyboard;
pub mod layout;
pub mod limine;
pub mod mkinitcpio;
pub mod nvidia_devices;
pub mod network;
pub mod powersave;
pub mod services;
pub mod snapshots;
pub mod validate;

pub use config::{Firmware, InstallConfig, Locale};
pub use desktop::Desktop;
pub use error::{Error, Result};
pub use events::{EventSink, InstallEvent, NullSink, StdoutSink};
