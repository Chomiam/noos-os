import { Card, Head } from "../components/primitives";
import { useI18n } from "../i18n";
import { formatBytes, type Disk } from "../lib/api";
import type { Draft } from "../lib/draft";
import { zoneCity } from "../components/WorldMap";

/**
 * Final review.
 *
 * Shows the destructive line last and in the danger tone: the point of this
 * screen is that the user reads the disk name one more time before it is
 * gone.
 */
export function SummaryScreen({
  draft,
  disks,
  drivers,
}: {
  draft: Draft;
  disks: Disk[];
  drivers: string[];
}) {
  const { t } = useI18n();

  const disk = disks.find((entry) => entry.device === draft.disk);
  const yes = t("common.yes");
  const no = t("common.no");

  const rows: [string, string][] = [
    [t("summary.language"), draft.locale === "fr" ? "Français" : "English"],
    [t("summary.timezone"), `${zoneCity(draft.timezone)} — ${draft.timezone}`],
    [t("summary.keyboard"), draft.keyboard],
    [t("summary.fullName"), draft.fullName],
    [t("summary.username"), draft.username],
    [t("summary.hostname"), draft.hostname],
    [t("summary.desktop"), t(`desktop.${draft.desktop}`)],
    [t("summary.kernel"), t(`options.kernel.${draft.kernel}`)],
    [t("summary.swap"), draft.swap ? yes : no],
  ];

  if (drivers.length > 0) {
    rows.push([t("summary.drivers"), drivers.join(", ")]);
  }

  return (
    <div className="fade-in">
      <Head title={t("summary.title")} subtitle={t("summary.subtitle")} />

      <div className="summary">
        {rows.map(([key, value]) => (
          <div className="summary__row" key={key}>
            <span className="summary__key">{key}</span>
            <span className="summary__value">{value}</span>
          </div>
        ))}

        <div className="summary__row">
          <span className="summary__key">{t("summary.disk")}</span>
          <span className="summary__value">
            {disk ? `${disk.model} — ${disk.device}` : draft.disk}
            {disk && (
              <span className="dim"> ({formatBytes(disk.sizeBytes)})</span>
            )}
          </span>
        </div>
      </div>

      <div style={{ marginTop: "var(--s-5)" }}>
        <Card tone="danger">{t("summary.warning", { disk: draft.disk })}</Card>
      </div>
    </div>
  );
}
