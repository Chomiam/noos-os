//! Tauri command surface.
//!
//! Everything here is a thin adapter: it runs the engine, converts errors to
//! strings the webview can display, and streams progress. No installation
//! logic lives in this crate — that is all in `noos-core`, which is why the
//! engine can be tested without a graphical session.

use noos_core::config::InstallConfig;
use noos_core::events::{EventSink, InstallEvent, NullSink};
use noos_core::exec::Runner;
use noos_core::install::Installer;
use noos_core::{gpu, hardware, keyboard, layout, network, validate};

use serde::Serialize;
use tauri::ipc::Channel;

/// Forwards engine events to the webview.
///
/// A `Channel` rather than Tauri's global event system: channels are ordered
/// and built for streams, while events are explicitly documented as not
/// intended for high-throughput situations — and pacstrap emits thousands of
/// lines that must arrive in order.
struct ChannelSink(Channel<InstallEvent>);

impl EventSink for ChannelSink {
    fn emit(&self, event: InstallEvent) {
        // A failed send means the window is gone; there is nothing useful to
        // do about it and the installation should not be aborted for it.
        let _ = self.0.send(event);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Hardware {
    gpus: Vec<gpu::Gpu>,
    codec_packages: Vec<String>,
    nvidia: Option<gpu::NvidiaPlan>,
    needs_headers: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasswordStrengthDto {
    score: u8,
    warning: Option<String>,
    suggestions: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PartitionPreview {
    number: u32,
    name: String,
    size_mib: u64,
    kind: &'static str,
}

#[tauri::command]
fn list_disks() -> Result<Vec<hardware::Disk>, String> {
    let sink = NullSink;
    let runner = Runner::new(&sink);
    hardware::list_disks(&runner).map_err(|error| error.to_string())
}

#[tauri::command]
fn detect_hardware() -> Result<Hardware, String> {
    let sink = NullSink;
    let runner = Runner::new(&sink);

    let gpus = gpu::detect(&runner).map_err(|error| error.to_string())?;
    let plan = gpu::codec_plan(&gpus);

    Ok(Hardware {
        gpus,
        codec_packages: plan.packages,
        nvidia: plan.nvidia,
        needs_headers: plan.needs_headers,
    })
}

#[tauri::command]
fn check_network() -> bool {
    let sink = NullSink;
    let runner = Runner::new(&sink);
    hardware::check_network(&runner)
}

/// The physical network interfaces of this machine.
#[tauri::command]
fn list_interfaces() -> Result<Vec<network::NetworkInterface>, String> {
    let sink = NullSink;
    let runner = Runner::new(&sink);
    network::list_interfaces(&runner).map_err(|error| error.to_string())
}

/// Whether the internet is reachable, and if not, why.
#[tauri::command]
fn connectivity() -> network::Connectivity {
    let sink = NullSink;
    let runner = Runner::new(&sink);
    network::connectivity(&runner)
}

/// Scans for wireless networks.
///
/// Takes a few seconds: `iwctl scan` returns as soon as the scan is started,
/// and iwd needs a moment to collect the results.
#[tauri::command]
async fn scan_wifi(interface: String) -> Result<Vec<network::WifiNetwork>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let sink = NullSink;
        let runner = Runner::new(&sink);
        network::scan_wifi(&runner, &interface).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("analyse interrompue : {error}"))?
}

/// Joins a wireless network, then waits for an address.
#[tauri::command]
async fn connect_wifi(
    interface: String,
    ssid: String,
    passphrase: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let sink = NullSink;
        let runner = Runner::new(&sink);

        network::connect_wifi(&runner, &interface, &ssid, passphrase.as_deref())
            .map_err(|error| error.to_string())?;

        // iwd only associates; `systemd-networkd` does the DHCP. Give it a
        // moment before the caller re-reads the interface state, or the UI
        // would show a joined network with no address and look broken.
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if network::list_interfaces(&runner)
                .unwrap_or_default()
                .iter()
                .any(|iface| iface.name == interface && iface.configured)
            {
                return Ok(());
            }
        }

        // Association succeeded but no address arrived. Not an error: the
        // network may simply be slow, and the user can carry on.
        Ok(())
    })
    .await
    .map_err(|error| format!("connexion interrompue : {error}"))?
}

/// The keyboard layouts the installer offers.
///
/// Served from the engine rather than duplicated in the frontend: the list
/// carries both the X11 layout and the console keymap for each entry, and two
/// copies of that would drift.
#[tauri::command]
fn list_keyboards() -> &'static [keyboard::Keyboard] {
    keyboard::KEYBOARDS
}

/// The layout to preselect for a time zone.
///
/// `countries` comes from `zone1970.tab`, which the frontend already has
/// compiled in — the zone's country codes are what make the guess possible.
#[tauri::command]
fn keyboard_for_countries(countries: Vec<String>) -> &'static keyboard::Keyboard {
    let codes: Vec<&str> = countries.iter().map(String::as_str).collect();
    keyboard::for_countries(&codes)
}

/// Evaluates a password for the strength meter.
///
/// Deliberately does not fail on a weak password: the meter has to show a
/// score for every keystroke, including bad ones. Enforcement happens in
/// [`validate_config`].
#[tauri::command]
fn check_password(password: String, user_inputs: Vec<String>) -> PasswordStrengthDto {
    let inputs: Vec<&str> = user_inputs.iter().map(String::as_str).collect();
    let strength = validate::password_strength(&password, &inputs);

    PasswordStrengthDto {
        score: strength.score.0,
        warning: strength.warning,
        suggestions: strength.suggestions,
    }
}

/// Returns `null` when the configuration is acceptable, or the reason it is
/// not. The reason is a message meant to be shown as-is.
#[tauri::command]
fn validate_config(config: InstallConfig) -> Option<String> {
    config.validate().err().map(|error| error.to_string())
}

/// The partition layout that would be written, for the summary screen.
#[tauri::command]
fn preview_layout(disk: String, swap: bool) -> Result<Vec<PartitionPreview>, String> {
    let sink = NullSink;
    let runner = Runner::new(&sink);

    let bytes: u64 = runner
        .capture_line("blockdev", &["--getsize64", &disk])
        .map_err(|error| error.to_string())?
        .parse()
        .map_err(|_| format!("taille de {disk} illisible"))?;

    let plan = layout::plan(bytes / (1024 * 1024), swap).map_err(|error| error.to_string())?;

    Ok(plan
        .all()
        .into_iter()
        .map(|part| PartitionPreview {
            number: part.number,
            name: part.name.clone(),
            size_mib: part.size_mib,
            kind: match part.kind {
                layout::PartitionKind::Esp => "esp",
                layout::PartitionKind::Btrfs => "btrfs",
                layout::PartitionKind::Swap => "swap",
            },
        })
        .collect())
}

/// Runs the installation, streaming progress to the caller.
///
/// Dispatched to a blocking thread: the engine shells out to pacstrap and
/// waits on it for minutes, which would otherwise freeze the async runtime.
#[tauri::command]
async fn start_install(
    config: InstallConfig,
    on_event: Channel<InstallEvent>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let sink = ChannelSink(on_event);
        Installer::new(config, &sink)
            .run()
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("tâche d'installation interrompue : {error}"))?
}

#[tauri::command]
fn reboot() -> Result<(), String> {
    // Sync first: the installation just wrote a lot, and systemd will not
    // flush on its own before an immediate reboot.
    let _ = std::process::Command::new("sync").status();

    std::process::Command::new("systemctl")
        .arg("reboot")
        .status()
        .map_err(|error| format!("redémarrage impossible : {error}"))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_disks,
            detect_hardware,
            check_network,
            list_keyboards,
            keyboard_for_countries,
            list_interfaces,
            connectivity,
            scan_wifi,
            connect_wifi,
            check_password,
            validate_config,
            preview_layout,
            start_install,
            reboot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
