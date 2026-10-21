//! noos-os installation engine.
//!
//! Deliberately free of any Tauri dependency: the Tauri app, a CLI and the
//! test suite all drive the same code. Anything that only *describes* the
//! install (partition sizes, `fstab`, `/etc/default/grub`, snapper settings) is
//! a pure function, so it can be verified without root; only [`exec`] and
//! [`install`] touch the machine.

pub mod audio;
pub mod btrfs;
pub mod chaotic;
pub mod config;
pub mod desktop;
pub mod error;
pub mod events;
pub mod exec;
pub mod flatpak;
pub mod gpu;
pub mod grub;
pub mod hardware;
pub mod install;
pub mod kernel;
pub mod keyboard;
pub mod layout;
pub mod locale;
pub mod mkinitcpio;
pub mod nvidia_devices;
pub mod network;
pub mod powersave;
pub mod secureboot;
pub mod services;
pub mod snapshots;
pub mod theme;
pub mod validate;

pub use config::{Firmware, InstallConfig, Locale};
pub use desktop::Desktop;
pub use error::{Error, Result};
pub use events::{EventSink, InstallEvent, NullSink, StdoutSink};
