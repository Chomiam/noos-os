//! Network inventory and Wi-Fi.
//!
//! Everything here is read-only except [`connect_wifi`]: the live medium is
//! already configured by `systemd-networkd` for wired links, so the installer
//! only has to *report* the state and, when there is no cable, help the user
//! join a wireless network.
//!
//! Parsing is separated from execution so it can be tested against captured
//! output — the live medium is not available in a test suite.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::exec::Runner;

/// What kind of link an interface is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Ethernet,
    Wifi,
    /// Bridges and veth pairs — libvirt's `virbr0`, containers, and so on.
    Virtual,
    Loopback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    pub name: String,
    pub kind: LinkKind,
    /// `up`, `down` or `unknown`, as `ip` reports it.
    pub state: String,
    /// Whether the link has carrier — a cable plugged in, or an association.
    ///
    /// Distinct from `state`: an interface can be administratively up with no
    /// cable, which is the difference between "nothing is plugged in" and
    /// "something is broken".
    pub carrier: bool,
    /// Addresses in CIDR form, IPv4 first.
    pub addresses: Vec<String>,
    /// True when the interface holds a global IPv4 address, i.e. it is
    /// actually usable.
    pub configured: bool,
}

#[derive(Debug, Deserialize)]
struct IpAddr {
    ifname: String,
    #[serde(default)]
    operstate: String,
    #[serde(default)]
    addr_info: Vec<IpAddrInfo>,
}

#[derive(Debug, Deserialize)]
struct IpAddrInfo {
    #[serde(default)]
    family: String,
    #[serde(default)]
    local: String,
    #[serde(default)]
    prefixlen: u8,
    /// Set for addresses that are not globally routable.
    #[serde(default)]
    scope: String,
}

/// Parses `ip -j addr show` output.
///
/// `is_wireless` is a callback rather than a path lookup so this stays pure
/// and testable: the caller passes something that checks
/// `/sys/class/net/<name>/wireless`.
pub fn parse_interfaces(json: &str, is_wireless: impl Fn(&str) -> bool) -> Result<Vec<NetworkInterface>> {
    let parsed: Vec<IpAddr> = serde_json::from_str(json)
        .map_err(|e| Error::Config(format!("sortie de ip illisible : {e}")))?;

    let interfaces = parsed
        .into_iter()
        .filter_map(|device| {
            let kind = classify(&device.ifname, is_wireless(&device.ifname));

            // Loopback and virtual bridges are noise in a wizard: the user is
            // choosing how to reach the internet, not auditing the host.
            if matches!(kind, LinkKind::Loopback | LinkKind::Virtual) {
                return None;
            }

            let mut addresses: Vec<String> = device
                .addr_info
                .iter()
                .filter(|info| !info.local.is_empty())
                .map(|info| format!("{}/{}", info.local, info.prefixlen))
                .collect();

            // IPv4 first: it is what the user will recognise, and the only
            // family that matters for reaching a mirror.
            addresses.sort_by_key(|address| !address.contains('.'));

            let configured = device.addr_info.iter().any(|info| {
                info.family == "inet" && info.scope != "host" && !info.local.is_empty()
            });

            Some(NetworkInterface {
                name: device.ifname,
                kind,
                state: if device.operstate.is_empty() {
                    "unknown".into()
                } else {
                    device.operstate.to_lowercase()
                },
                // Filled in by the caller, which can read sysfs.
                carrier: false,
                addresses,
                configured,
            })
        })
        .collect();

    Ok(interfaces)
}

/// Classifies an interface by its name, and by whether sysfs says it is
/// wireless.
fn classify(name: &str, wireless: bool) -> LinkKind {
    if name == "lo" {
        return LinkKind::Loopback;
    }
    if wireless {
        return LinkKind::Wifi;
    }
    // libvirt bridges, container veth pairs, and the like. Real Ethernet
    // devices start with `en` (predictable names) or `eth`.
    if name.starts_with("virbr")
        || name.starts_with("vnet")
        || name.starts_with("docker")
        || name.starts_with("br-")
        || name.starts_with("veth")
    {
        return LinkKind::Virtual;
    }
    LinkKind::Ethernet
}

/// Reads whether an interface has carrier, from sysfs.
fn has_carrier(name: &str) -> bool {
    std::fs::read_to_string(format!("/sys/class/net/{name}/carrier"))
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

fn is_wireless(name: &str) -> bool {
    std::path::Path::new(&format!("/sys/class/net/{name}/wireless")).exists()
}

/// Lists the physical network interfaces of this machine.
pub fn list_interfaces(runner: &Runner<'_>) -> Result<Vec<NetworkInterface>> {
    let json = runner.capture("ip", &["-j", "addr", "show"])?;
    let mut interfaces = parse_interfaces(&json, is_wireless)?;

    for interface in &mut interfaces {
        interface.carrier = has_carrier(&interface.name);
    }

    Ok(interfaces)
}

/// How well the machine can reach the outside world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Connectivity {
    /// Nothing is plugged in and no wireless network is joined.
    Offline,
    /// A link is up and has an address, but nothing answers.
    LinkOnly,
    /// The internet is reachable.
    Online,
}

/// A wireless network offered by a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiNetwork {
    pub ssid: String,
    /// Signal in dBm, or `None` when iwd did not report one.
    ///
    /// Kept as dBm rather than bars: the value is what the interface shows,
    /// and the frontend can turn it into bars without losing information.
    pub signal_dbm: Option<i32>,
    /// `open`, `psk`, `8021x` or iwd's own wording.
    pub security: String,
    pub connected: bool,
}

impl WifiNetwork {
    /// Whether joining this network needs a passphrase.
    pub fn needs_passphrase(&self) -> bool {
        let security = self.security.to_lowercase();
        !(security.is_empty() || security == "open" || security == "--")
    }
}

/// Parses the table `iwctl station <dev> get-networks rssi-dbms` prints.
///
/// There is no machine-readable output, so the table has to be read. Three
/// things make that survivable, and all three are handled below:
///
///  * iwd colours its output, so ANSI sequences have to be stripped;
///  * column widths vary with the longest network name;
///  * the `>` marker in the first column is how iwd shows the connected
///    network — it is not part of the name.
pub fn parse_wifi_networks(output: &str) -> Vec<WifiNetwork> {
    let plain = strip_ansi(output);
    let mut networks = Vec::new();

    for line in plain.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip the title, the rules, and the column header.
        if trimmed.starts_with("Available networks")
            || trimmed.starts_with("Network name")
            || trimmed.chars().all(|c| c == '-' || c.is_whitespace())
        {
            continue;
        }

        // iwd prefixes the connected network with '>'.
        let connected = trimmed.starts_with('>');
        let body = trimmed.trim_start_matches('>').trim();
        if body.is_empty() {
            continue;
        }

        // Split on runs of two or more spaces: network names may contain a
        // single space, but the columns are separated by at least two.
        let columns: Vec<&str> = body
            .split("  ")
            .map(str::trim)
            .filter(|column| !column.is_empty())
            .collect();

        let Some(ssid) = columns.first() else { continue };
        if ssid.is_empty() {
            continue;
        }

        let security = columns.get(1).copied().unwrap_or("").to_string();
        let signal_dbm = columns
            .get(2)
            .and_then(|column| column.split_whitespace().next())
            .and_then(|value| value.parse::<i32>().ok());

        networks.push(WifiNetwork {
            ssid: (*ssid).to_string(),
            signal_dbm,
            security,
            connected,
        });
    }

    // Strongest first: the network the user wants is almost always the one
    // they can actually reach.
    networks.sort_by_key(|network| std::cmp::Reverse(network.signal_dbm.unwrap_or(i32::MIN)));
    networks.dedup_by(|a, b| a.ssid == b.ssid);

    networks
}

/// Removes ANSI escape sequences from a string.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // An escape sequence ends at the first alphabetic byte after '['.
        if chars.next() == Some('[') {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }

    out
}

/// Scans for wireless networks on an interface.
///
/// Runs through `iwctl` rather than `iw`: `iwd` takes exclusive control of the
/// wireless device, and `iw scan` fails with "resource busy" while it is
/// running.
pub fn scan_wifi(runner: &Runner<'_>, interface: &str) -> Result<Vec<WifiNetwork>> {
    // The scan itself takes a few seconds; iwd returns immediately and the
    // results are only available once it has finished.
    runner.stream("iwctl", &["station", interface, "scan"])?;

    let output = runner.capture(
        "iwctl",
        &["station", interface, "get-networks", "rssi-dbms"],
    )?;

    Ok(parse_wifi_networks(&output))
}

/// Joins a wireless network.
///
/// The passphrase is passed as an argument rather than on stdin: `iwctl`
/// accepts no stdin form, and it is the one place in the installer where a
/// secret has to be visible in the process list. It is worth stating plainly —
/// the alternative is not supporting wireless at all.
pub fn connect_wifi(
    runner: &Runner<'_>,
    interface: &str,
    ssid: &str,
    passphrase: Option<&str>,
) -> Result<()> {
    let mut args = vec!["--dont-ask", "station", interface, "connect", ssid];

    if let Some(passphrase) = passphrase {
        args.push("--passphrase");
        args.push(passphrase);
    }

    runner.stream("iwctl", &args)
}

/// Whether the machine can reach the internet.
pub fn connectivity(runner: &Runner<'_>) -> Connectivity {
    if crate::hardware::check_network(runner) {
        return Connectivity::Online;
    }

    // No internet, but is anything plugged in? That distinction is what the
    // interface uses to decide between "connect a cable" and "check your
    // settings".
    let interfaces = list_interfaces(runner).unwrap_or_default();
    if interfaces.iter().any(|interface| interface.configured) {
        Connectivity::LinkOnly
    } else {
        Connectivity::Offline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed `ip -j addr show`, as `ip` emits it.
    const IP_JSON: &str = r#"[
      {"ifindex":1,"ifname":"lo","operstate":"UNKNOWN","addr_info":[
        {"family":"inet","local":"127.0.0.1","prefixlen":8,"scope":"host"},
        {"family":"inet6","local":"::1","prefixlen":128,"scope":"host"}]},
      {"ifindex":2,"ifname":"enp7s0","operstate":"UP","addr_info":[
        {"family":"inet","local":"192.168.1.57","prefixlen":24,"scope":"global"},
        {"family":"inet6","local":"fe80::1","prefixlen":64,"scope":"link"}]},
      {"ifindex":3,"ifname":"wlp8s0","operstate":"DOWN","addr_info":[]},
      {"ifindex":4,"ifname":"virbr0","operstate":"UP","addr_info":[
        {"family":"inet","local":"192.168.122.1","prefixlen":24,"scope":"global"}]}
    ]"#;

    fn no_wireless(_: &str) -> bool {
        false
    }

    #[test]
    fn lists_physical_interfaces_only() {
        // Loopback and libvirt's bridge are noise in a wizard.
        let interfaces = parse_interfaces(IP_JSON, no_wireless).unwrap();
        let names: Vec<&str> = interfaces.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["enp7s0", "wlp8s0"]);
    }

    #[test]
    fn detects_wireless_from_sysfs() {
        let interfaces =
            parse_interfaces(IP_JSON, |name| name == "wlp8s0").unwrap();

        let wifi = interfaces.iter().find(|i| i.name == "wlp8s0").unwrap();
        assert_eq!(wifi.kind, LinkKind::Wifi);

        let wired = interfaces.iter().find(|i| i.name == "enp7s0").unwrap();
        assert_eq!(wired.kind, LinkKind::Ethernet);
    }

    #[test]
    fn puts_ipv4_before_ipv6() {
        // The address a user recognises is the IPv4 one.
        let interfaces = parse_interfaces(IP_JSON, no_wireless).unwrap();
        let wired = interfaces.iter().find(|i| i.name == "enp7s0").unwrap();
        assert_eq!(wired.addresses[0], "192.168.1.57/24");
        assert!(wired.addresses[1].contains(':'));
    }

    #[test]
    fn a_link_local_address_does_not_count_as_configured() {
        // `configured` is what decides between "connect a cable" and "check
        // your settings", so a link-local-only interface must not read as
        // usable.
        let json = r#"[{"ifname":"enp1s0","operstate":"UP","addr_info":[
          {"family":"inet6","local":"fe80::5054:ff:fe0b:b5cc","prefixlen":64,"scope":"link"}]}]"#;
        let interfaces = parse_interfaces(json, no_wireless).unwrap();
        assert!(!interfaces[0].configured);
        assert_eq!(interfaces[0].addresses.len(), 1);
    }

    #[test]
    fn a_global_ipv4_address_counts_as_configured() {
        let interfaces = parse_interfaces(IP_JSON, no_wireless).unwrap();
        assert!(interfaces.iter().find(|i| i.name == "enp7s0").unwrap().configured);
        assert!(!interfaces.iter().find(|i| i.name == "wlp8s0").unwrap().configured);
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let json = r#"[{"ifname":"eth0"}]"#;
        let interfaces = parse_interfaces(json, no_wireless).unwrap();
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].state, "unknown");
        assert!(interfaces[0].addresses.is_empty());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_interfaces("pas du json", no_wireless).is_err());
    }

    /// Captured from `iwctl station wlan0 get-networks rssi-dbms`, with the
    /// ANSI colouring iwd applies.
    const IWCTL_OUTPUT: &str = "\u{1b}[1;90m                          Available networks\u{1b}[0m\n\
        \u{1b}[90m--------------------------------------------------------------------------------\u{1b}[0m\n\
        \u{1b}[1;90m  Network name                    Security            Signal\u{1b}[0m\n\
        \u{1b}[90m--------------------------------------------------------------------------------\u{1b}[0m\n\
        \u{1b}[1;90m> \u{1b}[0m\u{1b}[1;92mMonReseau\u{1b}[0m\u{1b}[1;90m                          psk                 -45 dBm\u{1b}[0m\n\
        \u{1b}[1;90m  Voisin\u{1b}[0m\u{1b}[1;90m                             psk                 -72 dBm\u{1b}[0m\n\
        \u{1b}[1;90m  CafeGratuit\u{1b}[0m\u{1b}[1;90m                       open                -60 dBm\u{1b}[0m\n";

    #[test]
    fn parses_the_iwctl_table() {
        let networks = parse_wifi_networks(IWCTL_OUTPUT);
        let names: Vec<&str> = networks.iter().map(|n| n.ssid.as_str()).collect();

        // Sorted strongest first.
        assert_eq!(names, vec!["MonReseau", "CafeGratuit", "Voisin"]);
    }

    #[test]
    fn reads_the_connected_marker() {
        // The '>' is iwd's marker, not part of the name.
        let networks = parse_wifi_networks(IWCTL_OUTPUT);
        let connected = networks.iter().find(|n| n.ssid == "MonReseau").unwrap();
        assert!(connected.connected);
        assert_eq!(connected.signal_dbm, Some(-45));

        let other = networks.iter().find(|n| n.ssid == "Voisin").unwrap();
        assert!(!other.connected);
    }

    #[test]
    fn reads_security_and_signal() {
        let networks = parse_wifi_networks(IWCTL_OUTPUT);
        let open = networks.iter().find(|n| n.ssid == "CafeGratuit").unwrap();
        assert_eq!(open.security, "open");
        assert_eq!(open.signal_dbm, Some(-60));
        assert!(!open.needs_passphrase());

        let secured = networks.iter().find(|n| n.ssid == "MonReseau").unwrap();
        assert_eq!(secured.security, "psk");
        assert!(secured.needs_passphrase());
    }

    #[test]
    fn handles_an_empty_scan() {
        let output = "                          Available networks\n\
            --------------------------------------------------------------------------------\n\
            \u{1b}[1;90m  Network name                    Security            Signal\u{1b}[0m\n\
            --------------------------------------------------------------------------------\n";
        assert!(parse_wifi_networks(output).is_empty());
    }

    #[test]
    fn handles_a_network_name_with_a_single_space() {
        // Columns are separated by runs of spaces, so a single space inside a
        // name must survive.
        let output = "  Network name                    Security            Signal\n\
            --------------------------------------------------------------------------------\n\
            \u{1b}[1;90m  Mon Beau Reseau                 psk                 -50 dBm\u{1b}[0m\n";
        let networks = parse_wifi_networks(output);
        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "Mon Beau Reseau");
    }

    #[test]
    fn strips_ansi_escapes() {
        let input = "\u{1b}[1;90mtexte\u{1b}[0m normal";
        assert_eq!(strip_ansi(input), "texte normal");
    }

    #[test]
    fn classifies_virtual_interfaces() {
        assert_eq!(classify("lo", false), LinkKind::Loopback);
        assert_eq!(classify("virbr0", false), LinkKind::Virtual);
        assert_eq!(classify("vnet3", false), LinkKind::Virtual);
        assert_eq!(classify("docker0", false), LinkKind::Virtual);
        assert_eq!(classify("enp7s0", false), LinkKind::Ethernet);
        assert_eq!(classify("wlp8s0", true), LinkKind::Wifi);
    }
}
