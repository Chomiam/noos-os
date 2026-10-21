/**
 * Typed wrappers around the Rust commands.
 *
 * The shapes here mirror the `serde` representations in `noos-core` exactly —
 * enums use the same `rename_all` casing, and the config uses camelCase. A
 * mismatch shows up as a deserialisation error at runtime, so the names are
 * worth keeping in one place.
 */

import { invoke, Channel } from "@tauri-apps/api/core";

export type Locale = "fr" | "en";
export type Desktop = "gnome" | "plasma" | "cinnamon" | "budgie";
export type Firmware = "uefi" | "bios";
export type NvidiaLegacy = "v580" | "v470" | "v390" | "v340";
export type NvidiaPlan = "openModules" | "legacy";
export type GpuVendor = "amd" | "nvidia" | "intel" | "unknown";
export type Kernel = "linux" | "lts" | "zen";

export type LinkKind = "ethernet" | "wifi" | "virtual" | "loopback";
export type Connectivity = "offline" | "linkOnly" | "online";

export interface NetworkInterface {
  name: string;
  kind: LinkKind;
  /** `up`, `down` or `unknown`, as `ip` reports it. */
  state: string;
  /** A cable is plugged in, or an association exists. */
  carrier: boolean;
  /** Addresses in CIDR form, IPv4 first. */
  addresses: string[];
  /** Holds a global IPv4 address, i.e. is actually usable. */
  configured: boolean;
}

export interface WifiNetwork {
  ssid: string;
  /** Signal in dBm, or null when iwd did not report one. */
  signalDbm: number | null;
  /** `open`, `psk`, `8021x`, … */
  security: string;
  connected: boolean;
}

/** A keyboard layout, as the engine lists them. */
export interface Keyboard {
  id: string;
  /** Display name, in the layout's own language. */
  label: string;
  x11: string;
  variant: string;
  console: string;
  /** ISO country codes this layout is the default for. */
  countries: string[];
}

export interface InstallConfig {
  locale: Locale;
  timezone: string;
  keyboard: string;
  hostname: string;
  fullName: string;
  username: string;
  password: string;
  desktop: Desktop;
  disk: string;
  swap: boolean;
  requireStrongPassword: boolean;
  firmware: Firmware;
  kernel: Kernel;
  nvidiaLegacy: NvidiaLegacy | null;
}

export interface Disk {
  device: string;
  model: string;
  sizeBytes: number;
  removable: boolean;
  /** True when a partition of this disk is mounted — the installer refuses those. */
  inUse: boolean;
}

export interface Gpu {
  vendor: GpuVendor;
  pciId: string;
  name: string;
}

export interface Hardware {
  gpus: Gpu[];
  /** Packages the installer will add for drivers and hardware video decode. */
  codecPackages: string[];
  nvidia: NvidiaPlan | null;
  needsHeaders: boolean;
  /** Every display adapter is virtual, so no vendor driver is installed. */
  virtualMachine: boolean;
}

export interface PasswordStrength {
  score: 0 | 1 | 2 | 3 | 4;
  /** Why the password is weak, when zxcvbn has something specific to say. */
  warning: string | null;
  suggestions: string[];
}

export interface PartitionPreview {
  number: number;
  name: string;
  sizeMib: number;
  kind: "esp" | "btrfs" | "swap";
}

// ---------------------------------------------------------------- commands

export const listDisks = () => invoke<Disk[]>("list_disks");

export const detectHardware = () => invoke<Hardware>("detect_hardware");

export const checkNetwork = () => invoke<boolean>("check_network");

export const listInterfaces = () => invoke<NetworkInterface[]>("list_interfaces");

/** Whether the internet is reachable, and if not, why. */
export const connectivity = () => invoke<Connectivity>("connectivity");

/**
 * Scans for wireless networks.
 *
 * Takes several seconds: `iwctl scan` returns as soon as the scan starts, and
 * iwd needs a moment to collect the results.
 */
export const scanWifi = (iface: string) =>
  invoke<WifiNetwork[]>("scan_wifi", { interface: iface });

/** Joins a wireless network and waits for an address. */
export const connectWifi = (
  iface: string,
  ssid: string,
  passphrase: string | null,
) => invoke<void>("connect_wifi", { interface: iface, ssid, passphrase });

export const listKeyboards = () => invoke<Keyboard[]>("list_keyboards");

/** The layout the engine would preselect for a set of country codes. */
export const keyboardForCountries = (countries: string[]) =>
  invoke<Keyboard>("keyboard_for_countries", { countries });

/**
 * Minimum score the installer accepts when "require a strong password" is on.
 * Mirrors `noos_core::validate::MIN_STRONG_SCORE`.
 */
export const MIN_STRONG_SCORE = 3;

export const checkPassword = (password: string, userInputs: string[]) =>
  invoke<PasswordStrength>("check_password", { password, userInputs });

/** Returns `null` when the config is valid, or the reason it is not. */
export const validateConfig = (config: InstallConfig) =>
  invoke<string | null>("validate_config", { config });

export const previewLayout = (disk: string, swap: boolean) =>
  invoke<PartitionPreview[]>("preview_layout", { disk, swap });

// ------------------------------------------------------------ installation

export type InstallEvent =
  | { event: "step"; data: { id: string; index: number; total: number; title: string } }
  | { event: "progress"; data: { percent: number | null; message: string } }
  | { event: "log"; data: { stream: "stdout" | "stderr"; line: string } }
  | { event: "finished"; data: { success: boolean; message: string } };

/**
 * Starts the installation, streaming events as they happen.
 *
 * Uses a `Channel` rather than Tauri's global event system: channels are
 * ordered and built for exactly this, while events are explicitly documented
 * as not intended for high-throughput streams — and pacstrap emits thousands
 * of lines.
 */
export function startInstall(
  config: InstallConfig,
  onEvent: (event: InstallEvent) => void,
): Promise<void> {
  const channel = new Channel<InstallEvent>();
  channel.onmessage = onEvent;
  return invoke<void>("start_install", { config, onEvent: channel });
}

export const reboot = () => invoke<void>("reboot");

// ------------------------------------------------------------------ format

export function formatBytes(bytes: number): string {
  const units = ["o", "Kio", "Mio", "Gio", "Tio"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  // Whole numbers for bytes, one decimal above: 512 Gio, not 512.0 Gio.
  return `${unit === 0 ? value : value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}
