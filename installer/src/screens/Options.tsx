import { useEffect, useState } from "react";

import { Badge, Card, Choice, Head, Switch } from "../components/primitives";
import { useI18n } from "../i18n";
import {
  checkNetwork,
  detectHardware,
  type Hardware,
  type Kernel,
  type NvidiaLegacy,
} from "../lib/api";
import type { Draft } from "../lib/draft";

/**
 * Options.
 *
 * Grouped into labelled sections rather than one flat list: by this point the
 * screen holds four unrelated decisions — the kernel, the optional services,
 * the detected hardware, and the package-management extras — and without
 * headings it reads as an undifferentiated wall of switches.
 */
export function OptionsScreen({
  draft,
  update,
}: {
  draft: Draft;
  update: (patch: Partial<Draft>) => void;
}) {
  const { t } = useI18n();
  const [hardware, setHardware] = useState<Hardware | null>(null);
  const [online, setOnline] = useState<boolean | null>(null);

  useEffect(() => {
    detectHardware().then(setHardware).catch(() => setHardware(null));
    checkNetwork().then(setOnline).catch(() => setOnline(false));
  }, []);

  // AUR packages have to be fetched and built, so the option is meaningless
  // without a network. Turning it off rather than hiding it explains why.
  const aurAvailable = online !== false;

  useEffect(() => {
    if (!aurAvailable && draft.enableAur) update({ enableAur: false });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [aurAvailable]);

  const legacy = hardware?.nvidia === "legacy";

  return (
    <div className="fade-in">
      <Head title={t("options.title")} subtitle={t("options.subtitle")} />

      <Section title={t("options.kernelSection")} hint={t("options.kernelHint")}>
        <div className="choices choices--three">
          {(["linux", "lts", "zen"] as Kernel[]).map((kernel) => (
            <Choice
              key={kernel}
              selected={draft.kernel === kernel}
              onSelect={() => update({ kernel })}
              title={t(`options.kernel.${kernel}`)}
              description={t(`options.kernel.${kernel}Desc`)}
            />
          ))}
        </div>
      </Section>

      <Section title={t("options.hardware")}>
        {hardware === null ? (
          <p className="dim">{t("common.loading")}</p>
        ) : hardware.gpus.length === 0 ? (
          <p>{t("options.noGpu")}</p>
        ) : (
          <div className="stack">
            {hardware.gpus.map((gpu) => (
              <div key={gpu.pciId} className="row row--between">
                <span>
                  {gpu.name}
                  <span className="mono dim" style={{ marginLeft: "var(--s-3)" }}>
                    {gpu.pciId}
                  </span>
                </span>
                <Badge tone={gpu.vendor === "unknown" ? "neutral" : "accent"}>
                  {gpu.vendor.toUpperCase()}
                </Badge>
              </div>
            ))}
          </div>
        )}

        {/* A virtual machine explains why no vendor driver is offered, which
         * would otherwise look like detection failing. */}
        {hardware?.virtualMachine && (
          <p style={{ marginTop: "var(--s-3)", fontSize: "var(--text-sm)" }}>
            {t("options.virtualMachine")}
          </p>
        )}

        {hardware?.nvidia === "openModules" && (
          <p style={{ marginTop: "var(--s-3)", fontSize: "var(--text-sm)" }}>
            {t("options.nvidiaOpen")}
          </p>
        )}

        {hardware && hardware.codecPackages.length > 0 && (
          <details style={{ marginTop: "var(--s-3)" }}>
            <summary
              style={{
                cursor: "pointer",
                fontSize: "var(--text-sm)",
                color: "var(--text-dim)",
              }}
            >
              {t("options.drivers")} ({hardware.codecPackages.length})
            </summary>
            <p
              className="mono"
              style={{ marginTop: "var(--s-2)", lineHeight: "var(--leading-loose)" }}
            >
              {hardware.codecPackages.join("  ")}
            </p>
          </details>
        )}

        {legacy && (
          <div style={{ marginTop: "var(--s-4)" }}>
            <p style={{ marginBottom: "var(--s-3)" }}>{t("options.nvidiaLegacy")}</p>

            <div className="choices choices--two">
              {(["v580", "v470", "v390", "v340"] as NvidiaLegacy[]).map((branch) => (
                <button
                  key={branch}
                  type="button"
                  className={`choice${draft.nvidiaLegacy === branch ? " choice--selected" : ""}`}
                  onClick={() =>
                    update({ nvidiaLegacy: draft.nvidiaLegacy === branch ? null : branch })
                  }
                >
                  <span className="choice__title">{branch.replace("v", "")}xx</span>
                  <span className="choice__desc">{t(`options.branch.${branch}`)}</span>
                </button>
              ))}
            </div>

            <div style={{ marginTop: "var(--s-3)" }}>
              <Card tone="warning">
                {draft.nvidiaLegacy
                  ? t("options.nvidiaWarning")
                  : t("options.nvidiaLegacyNone")}
              </Card>
            </div>
          </div>
        )}
      </Section>

      <Section title={t("options.softwareSection")}>
        <Switch
          checked={draft.enableAur}
          onChange={(enableAur) => update({ enableAur })}
          disabled={!aurAvailable}
          title={t("options.aur")}
          hint={aurAvailable ? t("options.aurHint") : t("options.networkRequired")}
        />
        <Switch
          checked={draft.installShelly}
          onChange={(installShelly) => update({ installShelly })}
          disabled={!aurAvailable || !draft.enableAur}
          title={t("options.shelly")}
          hint={t("options.shellyHint")}
        />
      </Section>
    </div>
  );
}

/** A labelled group of controls. */
function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="section">
      <h2 className="section__title">{title}</h2>
      {hint && <p className="section__hint">{hint}</p>}
      {children}
    </section>
  );
}
