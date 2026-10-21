use serde::{Deserialize, Serialize};

use crate::desktop::Desktop;
use crate::error::{Error, Result};
use crate::gpu::NvidiaLegacy;
use crate::kernel::Kernel;
use crate::validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    Fr,
    En,
}

impl Locale {
    /// Locale name written to `/etc/locale.gen` and `/etc/locale.conf`.
    pub fn glibc_name(self) -> &'static str {
        match self {
            Locale::Fr => "fr_FR.UTF-8",
            Locale::En => "en_US.UTF-8",
        }
    }

    /// Short tag used to pick translation files.
    pub fn tag(self) -> &'static str {
        match self {
            Locale::Fr => "fr",
            Locale::En => "en",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Firmware {
    Uefi,
    Bios,
}

/// Everything the user chose in the installer UI, in one serialisable struct.
///
/// This is what crosses the Tauri IPC boundary. The password is part of it
/// because the frontend has to send it, but `Debug` is implemented by hand so
/// it can never leak into a log line.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallConfig {
    pub locale: Locale,
    /// IANA zone, e.g. `Europe/Paris`.
    pub timezone: String,
    /// Keyboard layout id, as listed by [`crate::keyboard::KEYBOARDS`].
    ///
    /// Distinct from the locale: the language someone reads and the keyboard
    /// they type on are independent choices, and an installer that derives one
    /// from the other gets it wrong for anyone who is not typical.
    pub keyboard: String,
    pub hostname: String,
    pub full_name: String,
    pub username: String,
    pub password: String,
    pub desktop: Desktop,
    /// Block device of the target disk, e.g. `/dev/nvme0n1`.
    pub disk: String,
    pub swap: bool,
    pub require_strong_password: bool,
    pub firmware: Firmware,
    /// Which kernel to install. Drives the package, the headers, the initramfs
    /// presets and the kernel paths the boot entry names.
    pub kernel: Kernel,
    /// Which legacy NVIDIA branch to build, when the detected card predates
    /// Turing.
    ///
    /// Detection can tell that a card is *legacy*, but not which branch it
    /// needs — NVIDIA's support windows are keyed to architecture, and the
    /// device-ID ranges overlap. Since every legacy branch is an AUR package
    /// anyway, the user picks; `None` means no proprietary driver.
    #[serde(default)]
    pub nvidia_legacy: Option<NvidiaLegacy>,
}

impl std::fmt::Debug for InstallConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallConfig")
            .field("locale", &self.locale)
            .field("timezone", &self.timezone)
            .field("keyboard", &self.keyboard)
            .field("hostname", &self.hostname)
            .field("full_name", &self.full_name)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("desktop", &self.desktop)
            .field("disk", &self.disk)
            .field("swap", &self.swap)
            .field("require_strong_password", &self.require_strong_password)
            .field("firmware", &self.firmware)
            .field("kernel", &self.kernel)
            .field("nvidia_legacy", &self.nvidia_legacy)
            .finish()
    }
}

impl InstallConfig {
    /// Name of the boot menu entry and distributor identifier (`GRUB_DISTRIBUTOR`).
    pub const OS_ENTRY_NAME: &'static str = "noos-os";

    /// Everything that can be checked before touching a single byte on disk.
    pub fn validate(&self) -> Result<()> {
        validate::username(&self.username)
            .map_err(|e| Error::Config(e.to_string()))?;
        validate::hostname(&self.hostname)
            .map_err(|e| Error::Config(e.to_string()))?;
        validate::timezone(&self.timezone)
            .map_err(|e| Error::Config(e.to_string()))?;

        if crate::keyboard::by_id(&self.keyboard).is_none() {
            return Err(Error::Config(format!(
                "disposition de clavier inconnue : {}",
                self.keyboard
            )));
        }

        if self.full_name.trim().is_empty() {
            return Err(Error::Config("le nom complet est vide".into()));
        }
        if !self.disk.starts_with("/dev/") {
            return Err(Error::Config(format!(
                "chemin de disque invalide : {}",
                self.disk
            )));
        }

        // The password policy is enforced here rather than through PAM: the
        // installer writes the hash directly with chpasswd, which bypasses
        // pam_pwquality entirely. If we do not check, nobody does.
        let inputs = [self.username.as_str(), self.full_name.as_str()];
        validate::password(&self.password, &inputs, self.require_strong_password)
            .map_err(|e| Error::Config(e.to_string()))?;

        Ok(())
    }
}
