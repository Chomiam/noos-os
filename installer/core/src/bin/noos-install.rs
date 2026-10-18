//! Command-line front-end for the noos-os installation engine.
//!
//! Exists so the engine can be exercised without a graphical session — and so
//! that the parts which do not need root (planning, validation, generated
//! configuration, hardware detection) are usable on their own.

use std::path::PathBuf;
use std::process::ExitCode;

use noos_core::config::InstallConfig;
use noos_core::events::StdoutSink;
use noos_core::exec::Runner;
use noos_core::install::Installer;
use noos_core::{btrfs, gpu, keyboard, layout, mkinitcpio, refind};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("plan") => cmd_plan(&args[1..]),
        Some("gpu") => cmd_gpu(),
        Some("check") => cmd_check(&args[1..]),
        Some("install") => cmd_install(&args[1..]),
        Some("-h" | "--help") | None => {
            usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("commande inconnue : {other}\n");
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    println!(
        "noos-install — moteur d'installation noos-os\n\
         \n\
         USAGE:\n\
         \x20   noos-install plan <taille-du-disque-en-Gio> [--swap]\n\
         \x20   noos-install gpu\n\
         \x20   noos-install check <config.json>\n\
         \x20   noos-install install <config.json>\n\
         \n\
         plan     affiche le plan de partitions, sans rien toucher\n\
         gpu      détecte le matériel graphique et la pile de codecs retenue\n\
         check    valide une configuration et montre les fichiers générés\n\
         install  exécute l'installation (nécessite root)\n\
         \n\
         OPTIONS:\n\
         \x20   --swap    inclure une partition d'échange de 8 Gio\n"
    );
}

/// Prints the partition layout for a disk of the given size.
fn cmd_plan(args: &[String]) -> ExitCode {
    let Some(size) = args.first() else {
        eprintln!("plan : taille du disque manquante");
        return ExitCode::FAILURE;
    };

    let Ok(gib) = size.parse::<f64>() else {
        eprintln!("plan : « {size} » n'est pas un nombre");
        return ExitCode::FAILURE;
    };
    if gib <= 0.0 {
        eprintln!("plan : la taille doit être positive");
        return ExitCode::FAILURE;
    }

    let with_swap = args[1..].iter().any(|a| a == "--swap");
    let disk_mib = (gib * 1024.0).round() as u64;

    match layout::plan(disk_mib, with_swap) {
        Ok(plan) => {
            println!(
                "Plan pour un disque de {gib:.1} Gio (swap : {})\n",
                if with_swap { "oui" } else { "non" }
            );
            println!(
                "{:<4} {:<12} {:>12} {:>12}  {}",
                "#", "nom", "début (Mio)", "taille (Mio)", "type"
            );
            for part in plan.all() {
                println!(
                    "{:<4} {:<12} {:>12} {:>12}  {:?}",
                    part.number, part.name, part.start_mib, part.size_mib, part.kind
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

/// Detects the display hardware of the machine this runs on.
///
/// Works on a normal desktop as well as on the live medium, which makes it the
/// quickest way to check the detection against real hardware.
fn cmd_gpu() -> ExitCode {
    let sink = StdoutSink;
    let runner = Runner::new(&sink);

    let gpus = match gpu::detect(&runner) {
        Ok(gpus) => gpus,
        Err(err) => {
            eprintln!("détection impossible : {err}");
            return ExitCode::FAILURE;
        }
    };

    if gpus.is_empty() {
        println!("Aucun adaptateur graphique détecté.");
    } else {
        println!("Adaptateurs détectés :");
        for gpu in &gpus {
            println!(
                "  {:<8} {} [{}]",
                gpu.vendor.label(),
                gpu.name,
                gpu.pci_id
            );
        }
    }

    let plan = gpu::codec_plan(&gpus);
    match plan.nvidia {
        Some(gpu::NvidiaPlan::OpenModules) => {
            println!("\nPilote NVIDIA : modules ouverts (Turing ou plus récent)");
        }
        Some(gpu::NvidiaPlan::Legacy) => {
            println!(
                "\nPilote NVIDIA : carte antérieure à Turing.\n\
                 Branches possibles (toutes dans l'AUR) :"
            );
            for branch in gpu::NvidiaLegacy::all() {
                println!("  {:<20} {}", branch.package(), branch.label());
            }
        }
        None => {}
    }

    println!("\nPaquets retenus ({}) :", plan.packages.len());
    for package in &plan.packages {
        println!("  {package}");
    }
    if plan.needs_headers {
        println!("\n-> pilote DKMS : linux-headers est requis");
    }

    ExitCode::SUCCESS
}

/// Validates a configuration and prints every file the installer would write.
///
/// Useful for reviewing a configuration before committing to a disk wipe, and
/// as a smoke test on a machine where running the real thing is not an option.
fn cmd_check(args: &[String]) -> ExitCode {
    let config = match load_config(args) {
        Ok(config) => config,
        Err(code) => return code,
    };

    if let Err(err) = config.validate() {
        eprintln!("configuration refusée : {err}");
        return ExitCode::FAILURE;
    }
    println!("Configuration valide.\n");

    // Placeholder UUIDs: the shape of the generated files is what matters here.
    let btrfs_uuid = "<uuid-btrfs>";
    let esp_uuid = "<uuid-esp>";
    let swap_uuid = config.swap.then_some("<uuid-swap>");

    println!(
        "===== /etc/fstab =====\n{}",
        btrfs::fstab(btrfs_uuid, esp_uuid, swap_uuid, false)
    );
    println!(
        "===== /boot/EFI/refind/refind.conf =====\n{}",
        refind::refind_conf(config.kernel)
    );
    println!(
        "===== /boot/refind_linux.conf =====\n{}",
        refind::refind_linux_conf(btrfs_uuid, swap_uuid)
    );
    println!(
        "===== themes/noos/theme.conf =====\n{}",
        refind::theme_conf()
    );
    println!(
        "===== /etc/mkinitcpio.conf =====\n{}",
        mkinitcpio::config(config.swap)
    );

    if let Some(keyboard) = keyboard::by_id(&config.keyboard) {
        println!(
            "===== /etc/vconsole.conf =====\n{}",
            keyboard::vconsole_conf(keyboard)
        );
        println!(
            "===== /etc/X11/xorg.conf.d/00-keyboard.conf =====\n{}",
            keyboard::x11_conf(keyboard)
        );
    }
    println!(
        "===== paquets du bureau ({}) =====\n{}",
        config.desktop.packages().len(),
        config.desktop.packages().join(" ")
    );
    println!(
        "===== gestionnaire de connexion =====\n{}",
        config.desktop.display_manager().service()
    );

    ExitCode::SUCCESS
}

/// Runs the real installation. Requires root.
fn cmd_install(args: &[String]) -> ExitCode {
    let config = match load_config(args) {
        Ok(config) => config,
        Err(code) => return code,
    };

    let sink = StdoutSink;
    match Installer::new(config, &sink).run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("\nÉchec de l'installation : {err}");
            ExitCode::FAILURE
        }
    }
}

fn load_config(args: &[String]) -> Result<InstallConfig, ExitCode> {
    let Some(path) = args.first() else {
        eprintln!("chemin du fichier de configuration manquant");
        return Err(ExitCode::FAILURE);
    };
    let path = PathBuf::from(path);

    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            eprintln!("lecture de {} impossible : {err}", path.display());
            return Err(ExitCode::FAILURE);
        }
    };

    match serde_json::from_str(&raw) {
        Ok(config) => Ok(config),
        Err(err) => {
            eprintln!(
                "{} n'est pas une configuration valide : {err}",
                path.display()
            );
            Err(ExitCode::FAILURE)
        }
    }
}
