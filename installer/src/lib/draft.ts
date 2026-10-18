/**
 * The draft configuration, and the client-side validation that mirrors it.
 *
 * The rules are duplicated from `noos_core::validate` on purpose: Rust stays
 * authoritative — it is what actually runs before anything touches the disk —
 * but waiting on a round trip for every keystroke would make the form feel
 * broken. The two must agree, and the Rust tests are what pin the real
 * behaviour.
 */

import type {
  Desktop,
  Firmware,
  InstallConfig,
  Kernel,
  Locale,
  NvidiaLegacy,
} from "./api";

/** The configuration as the user is still building it. */
export interface Draft {
  locale: Locale;
  timezone: string;
  /** Keyboard layout id, chosen independently of the language. */
  keyboard: string;
  hostname: string;
  fullName: string;
  username: string;
  password: string;
  /** Kept only for the form; never sent anywhere. */
  passwordConfirm: string;
  desktop: Desktop;
  disk: string;
  swap: boolean;
  requireStrongPassword: boolean;
  enableAur: boolean;
  installShelly: boolean;
  nvidiaLegacy: NvidiaLegacy | null;
  firmware: Firmware;
  kernel: Kernel;
}

export const INITIAL_DRAFT: Draft = {
  locale: "fr",
  timezone: "Europe/Paris",
  // Preselected from the zone once the keyboard list is loaded; this is only
  // the value used before that happens.
  keyboard: "us",
  hostname: "noos",
  fullName: "",
  username: "",
  password: "",
  passwordConfirm: "",
  desktop: "plasma",
  disk: "",
  swap: true,
  requireStrongPassword: true,
  enableAur: true,
  installShelly: true,
  nvidiaLegacy: null,
  firmware: "uefi",
  kernel: "linux",
};

/** Strips the form-only fields and produces what the engine expects. */
export function toInstallConfig(draft: Draft): InstallConfig {
  return {
    locale: draft.locale,
    timezone: draft.timezone,
    keyboard: draft.keyboard,
    hostname: draft.hostname,
    fullName: draft.fullName,
    username: draft.username,
    password: draft.password,
    desktop: draft.desktop,
    disk: draft.disk,
    swap: draft.swap,
    requireStrongPassword: draft.requireStrongPassword,
    enableAur: draft.enableAur,
    installShelly: draft.installShelly,
    firmware: draft.firmware,
    kernel: draft.kernel,
    nvidiaLegacy: draft.nvidiaLegacy,
  };
}

// ------------------------------------------------------------- validation ---

export const USERNAME_MAX_LEN = 32;

const USERNAME_PATTERN = /^[a-z_][a-z0-9_-]*$/;
const HOSTNAME_PATTERN = /^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/;

/**
 * Names that already exist on a stock Arch install.
 *
 * Mirrors the list in `noos_core::validate::RESERVED_USERNAMES`.
 */
const RESERVED = new Set([
  "root", "bin", "daemon", "mail", "ftp", "http", "nobody", "adm", "lp",
  "sync", "shutdown", "halt", "operator", "games", "news", "uucp", "proxy",
  "backup", "list", "irc", "man", "sys", "tty", "disk", "kmem", "wheel",
  "utmp", "locate", "rfkill", "users", "audio", "video", "optical", "storage",
  "power", "input", "kvm", "render", "dbus", "polkitd", "rtkit", "avahi",
  "colord", "cups", "geoclue", "git", "tss", "usbmux", "uuidd", "sddm",
  "lightdm", "gdm", "greeter", "admin", "administrator", "guest", "test",
  "user", "mysql", "postgres", "postfix", "nginx", "apache", "systemd",
]);

/**
 * Returns the i18n key of the error, or `null` when the value is acceptable.
 * The caller passes the key through `t()` and can supply the placeholders.
 */
export interface FieldError {
  key: string;
  vars?: Record<string, string | number>;
}

export function validateUsername(value: string): FieldError | null {
  if (!value) return { key: "account.error.usernameEmpty" };

  const length = [...value].length;
  if (length > USERNAME_MAX_LEN) {
    return {
      key: "account.error.usernameTooLong",
      vars: { max: USERNAME_MAX_LEN, len: length },
    };
  }
  if (!USERNAME_PATTERN.test(value)) return { key: "account.error.usernameIllegal" };
  if (RESERVED.has(value)) {
    return { key: "account.error.usernameReserved", vars: { name: value } };
  }
  return null;
}

export function validateHostname(value: string): FieldError | null {
  if (!value) return { key: "account.error.empty" };
  if ([...value].length > 63) return { key: "account.error.hostnameTooLong" };
  if (!HOSTNAME_PATTERN.test(value)) return { key: "account.error.hostnameIllegal" };
  return null;
}

export function validateFullName(value: string): FieldError | null {
  return value.trim() ? null : { key: "account.error.empty" };
}

/**
 * Whether the account step can be left.
 *
 * Password *strength* is deliberately not checked here: it is evaluated
 * asynchronously in Rust, and the meter already reports it inline. The engine
 * enforces it for real when the configuration is submitted — this only gates
 * the fields whose rules are knowable instantly.
 */
export function isAccountComplete(draft: Draft): boolean {
  return (
    !validateFullName(draft.fullName) &&
    !validateUsername(draft.username) &&
    !validateHostname(draft.hostname) &&
    draft.password.length > 0 &&
    draft.password === draft.passwordConfirm
  );
}
