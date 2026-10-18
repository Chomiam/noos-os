import { useCallback, useEffect, useState } from "react";

import { Badge, Button, Card, Head } from "../components/primitives";
import { Alert, Check, Globe } from "../components/icons";
import { useI18n } from "../i18n";
import {
  connectivity,
  connectWifi,
  listInterfaces,
  scanWifi,
  type Connectivity,
  type NetworkInterface,
  type WifiNetwork,
} from "../lib/api";

/**
 * Network.
 *
 * The tone matters more here than anywhere else in the installer. A machine
 * with no network is not broken and the installation does not need one — only
 * the AUR step does, and it is optional. So this screen never reports a
 * failure: it says what it found, and offers a way forward.
 *
 * Wired links are already handled by `systemd-networkd` before the interface
 * starts, so there is nothing to configure. Wireless is the only thing that
 * needs the user, and it is only offered when the machine actually has a
 * wireless interface.
 */
export function NetworkScreen() {
  const { t } = useI18n();

  const [state, setState] = useState<Connectivity>("offline");
  const [interfaces, setInterfaces] = useState<NetworkInterface[]>([]);
  const [networks, setNetworks] = useState<WifiNetwork[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [passphraseFor, setPassphraseFor] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listInterfaces().then(setInterfaces).catch(() => setInterfaces([]));
    connectivity().then(setState).catch(() => setState("offline"));
  }, []);

  useEffect(() => {
    refresh();
    // A wired link can come up a second or two after the interface opens —
    // DHCP is asynchronous, and the user may have just plugged a cable in.
    const timer = setInterval(refresh, 4000);
    return () => clearInterval(timer);
  }, [refresh]);

  const wifiInterface = interfaces.find((iface) => iface.kind === "wifi");
  const wiredInterfaces = interfaces.filter((iface) => iface.kind === "ethernet");

  const scan = async () => {
    if (!wifiInterface) return;
    setScanning(true);
    setError(null);
    try {
      setNetworks(await scanWifi(wifiInterface.name));
    } catch (reason) {
      setError(String(reason));
      setNetworks([]);
    } finally {
      setScanning(false);
    }
  };

  const join = async (network: WifiNetwork) => {
    if (!wifiInterface) return;

    if (network.security === "open" || network.security === "--") {
      await doConnect(network.ssid, null);
      return;
    }
    // Ask for the passphrase first.
    setPassphraseFor(network.ssid);
    setPassphrase("");
  };

  const doConnect = async (ssid: string, secret: string | null) => {
    if (!wifiInterface) return;
    setConnecting(ssid);
    setError(null);
    setPassphraseFor(null);
    try {
      await connectWifi(wifiInterface.name, ssid, secret);
      refresh();
      if (networks) setNetworks(await scanWifi(wifiInterface.name));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setConnecting(null);
      setPassphrase("");
    }
  };

  return (
    <div className="fade-in">
      <Head title={t("network.title")} subtitle={t("network.subtitle")} />

      {/* The overall state, as a single calm statement. */}
      <StatusBanner state={state} />

      {/* Wired. Usually nothing to do — which is exactly what to say. */}
      {wiredInterfaces.length > 0 && (
        <section className="section">
          <h2 className="section__title">{t("network.wired")}</h2>
          {wiredInterfaces.map((iface) => (
            <InterfaceRow key={iface.name} iface={iface} />
          ))}
          {!wiredInterfaces.some((iface) => iface.configured) && (
            <p className="net-hint">{t("network.wiredHint")}</p>
          )}
        </section>
      )}

      {/* Wireless, only when the machine has it. */}
      {wifiInterface ? (
        <section className="section">
          <h2 className="section__title">{t("network.wireless")}</h2>
          <InterfaceRow iface={wifiInterface} />

          <div className="net-actions">
            <Button variant="outline" onClick={() => void scan()} disabled={scanning}>
              {scanning ? t("network.scanning") : t("network.scan")}
            </Button>
          </div>

          {networks !== null && networks.length === 0 && !scanning && (
            <p className="net-hint">{t("network.noNetwork")}</p>
          )}

          {networks && networks.length > 0 && (
            <ul className="wifi-list">
              {networks.map((network) => (
                <li key={network.ssid}>
                  <button
                    type="button"
                    className={`wifi${network.connected ? " wifi--on" : ""}`}
                    onClick={() => void join(network)}
                    disabled={connecting !== null}
                  >
                    <SignalBars dbm={network.signalDbm} />

                    <span className="wifi__name">{network.ssid}</span>

                    {network.connected && (
                      <Badge tone="success">
                        <Check size={12} />
                        {t("network.connected")}
                      </Badge>
                    )}

                    {connecting === network.ssid ? (
                      <Badge tone="accent">{t("network.connecting")}</Badge>
                    ) : network.security === "open" || network.security === "--" ? (
                      <Badge>{t("network.open")}</Badge>
                    ) : (
                      <span className="wifi__lock" title={t("network.secured")}>
                        🔒
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}

          {/* The passphrase prompt, inline rather than a modal: a dialog over
           * a fullscreen kiosk is one more thing that can go wrong. */}
          {passphraseFor && (
            <div className="net-pass">
              <label className="field__label" htmlFor="wifi-pass">
                {t("network.passphraseFor", { ssid: passphraseFor })}
              </label>
              <div className="net-pass__row">
                <input
                  id="wifi-pass"
                  className="field__input"
                  type="password"
                  value={passphrase}
                  autoFocus
                  onChange={(event) => setPassphrase(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && passphrase) {
                      void doConnect(passphraseFor, passphrase);
                    }
                  }}
                />
                <Button
                  variant="primary"
                  disabled={!passphrase}
                  onClick={() => void doConnect(passphraseFor, passphrase)}
                >
                  {t("network.join")}
                </Button>
              </div>
            </div>
          )}
        </section>
      ) : (
        <p className="net-hint">{t("network.noWifi")}</p>
      )}

      {error && (
        <div style={{ marginTop: "var(--s-4)" }}>
          <Card tone="warning" title={t("network.trouble")}>
            {error}
          </Card>
        </div>
      )}

      {/* The reassurance. This is the last thing on the screen on purpose. */}
      <p className="net-footnote">{t("network.optional")}</p>
    </div>
  );
}

/**
 * The overall state, worded as an observation rather than a verdict.
 *
 * "No connection" is a fact, not an error: the installation does not need the
 * network, and presenting it in red would make the user think they have to
 * fix something before they can carry on.
 */
function StatusBanner({ state }: { state: Connectivity }) {
  const { t } = useI18n();

  if (state === "online") {
    return (
      <div className="net-status net-status--on">
        <Globe size={20} />
        <div>
          <div className="net-status__title">{t("network.stateOnline")}</div>
          <div className="net-status__detail">{t("network.stateOnlineDetail")}</div>
        </div>
      </div>
    );
  }

  if (state === "linkOnly") {
    return (
      <div className="net-status net-status--partial">
        <Alert size={20} />
        <div>
          <div className="net-status__title">{t("network.stateLinkOnly")}</div>
          <div className="net-status__detail">{t("network.stateLinkOnlyDetail")}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="net-status">
      <Alert size={20} />
      <div>
        <div className="net-status__title">{t("network.stateOffline")}</div>
        <div className="net-status__detail">{t("network.stateOfflineDetail")}</div>
      </div>
    </div>
  );
}

/** One interface, described in the terms a user would use. */
function InterfaceRow({ iface }: { iface: NetworkInterface }) {
  const { t } = useI18n();

  const state = iface.configured
    ? "connected"
    : iface.carrier
      ? "connecting"
      : iface.state === "up"
        ? "noCable"
        : "down";

  const tone = iface.configured ? "success" : iface.carrier ? "accent" : "neutral";

  return (
    <div className="net-iface">
      <span className={`net-iface__dot net-iface__dot--${tone}`} />
      <span className="net-iface__name">{iface.name}</span>
      <span className="net-iface__state">{t(`network.iface.${state}`)}</span>
      {iface.addresses[0] && (
        <span className="net-iface__addr mono">{iface.addresses[0]}</span>
      )}
    </div>
  );
}

/**
 * Signal strength as four bars.
 *
 * Drawn from the dBm value rather than taken from iwd's own `rssi-bars`
 * output: the number is what the backend has, and rendering it here keeps the
 * thresholds in one place.
 */
function SignalBars({ dbm }: { dbm: number | null }) {
  // Roughly: -50 dBm is excellent, -90 is unusable. Anything below -85 gets
  // one bar rather than none, so the network still looks selectable.
  const level = dbm === null ? 2 : dbm >= -55 ? 4 : dbm >= -67 ? 3 : dbm >= -78 ? 2 : 1;

  return (
    <span className="bars" aria-hidden="true">
      {[1, 2, 3, 4].map((bar) => (
        <span
          key={bar}
          className={`bars__bar${bar <= level ? " bars__bar--on" : ""}`}
          style={{ height: `${3 + bar * 2.5}px` }}
        />
      ))}
    </span>
  );
}
