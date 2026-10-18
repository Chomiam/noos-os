import { useEffect, useState } from "react";

import { Badge, Card, Head, Switch } from "../components/primitives";
import { Disk as DiskIcon } from "../components/icons";
import { useI18n } from "../i18n";
import { formatBytes, previewLayout, type Disk, type PartitionPreview } from "../lib/api";
import type { Draft } from "../lib/draft";

/**
 * Disk selection.
 *
 * The most dangerous screen in the installer, so it is the most explicit one:
 * disks that are in use are shown but cannot be selected, the exact layout is
 * previewed before anything is written, and the confirmation has to be turned
 * on deliberately.
 *
 * The disk list is passed in rather than loaded here — the summary screen
 * needs the same data, and querying lsblk twice for it would be waste.
 */
export function DiskScreen({
  draft,
  update,
  disks,
  confirmed,
  setConfirmed,
}: {
  draft: Draft;
  update: (patch: Partial<Draft>) => void;
  disks: Disk[];
  confirmed: boolean;
  setConfirmed: (value: boolean) => void;
}) {
  const { t } = useI18n();
  const [layout, setLayout] = useState<PartitionPreview[]>([]);

  // Preselect the first usable disk: making someone click through an obvious
  // single-disk machine adds nothing.
  useEffect(() => {
    if (draft.disk) return;
    const firstFree = disks.find((disk) => !disk.inUse);
    if (firstFree) update({ disk: firstFree.device });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [disks]);

  useEffect(() => {
    if (!draft.disk) {
      setLayout([]);
      return;
    }
    previewLayout(draft.disk, draft.swap)
      .then(setLayout)
      .catch(() => setLayout([]));
  }, [draft.disk, draft.swap]);

  // A confirmation given for one disk must not carry over to another.
  useEffect(() => {
    setConfirmed(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft.disk]);

  const selected = disks.find((disk) => disk.device === draft.disk);

  return (
    <div className="fade-in">
      <Head title={t("disk.title")} subtitle={t("disk.subtitle")} />

      {disks.length === 0 ? (
        <div className="empty">
          <p>{t("disk.noDisk")}</p>
          <p style={{ marginTop: "var(--s-2)", fontSize: "var(--text-sm)" }}>
            {t("disk.noDiskHint")}
          </p>
        </div>
      ) : (
        disks.map((disk) => (
          <button
            key={disk.device}
            type="button"
            disabled={disk.inUse}
            className={[
              "disk",
              draft.disk === disk.device ? "disk--selected" : "",
              disk.inUse ? "disk--disabled" : "",
            ]
              .filter(Boolean)
              .join(" ")}
            onClick={() => update({ disk: disk.device })}
          >
            <DiskIcon size={20} />
            <span className="disk__info">
              <span className="disk__model">{disk.model}</span>
              <span className="disk__device">{disk.device}</span>
            </span>
            {disk.inUse ? (
              <Badge tone="warning">{t("disk.inUse")}</Badge>
            ) : disk.removable ? (
              <Badge>USB</Badge>
            ) : null}
            <span className="disk__size">{formatBytes(disk.sizeBytes)}</span>
          </button>
        ))
      )}

      {selected && (
        <>
          <div style={{ marginTop: "var(--s-6)" }}>
            <Switch
              checked={draft.swap}
              onChange={(swap) => update({ swap })}
              title={t("disk.swap")}
              hint={t("disk.swapHint")}
            />
          </div>

          {layout.length > 0 && (
            <div className="card" style={{ marginTop: "var(--s-4)" }}>
              <div className="card__title">{t("disk.layout")}</div>
              <div className="layout-list">
                {layout.map((part) => (
                  <div key={part.number} className="layout-row">
                    <span className="layout-row__num">{part.number}</span>
                    <span className="layout-row__kind">
                      {t(`disk.layout${part.kind[0].toUpperCase()}${part.kind.slice(1)}`)}
                    </span>
                    <span className="layout-row__size">
                      {formatBytes(part.sizeMib * 1024 * 1024)}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div style={{ marginTop: "var(--s-5)" }}>
            <Card tone="danger" title={t("disk.warningTitle")}>
              {t("disk.warningBody")}
            </Card>

            <div style={{ marginTop: "var(--s-4)" }}>
              <Switch
                checked={confirmed}
                onChange={setConfirmed}
                title={t("disk.confirmLabel", { disk: selected.device })}
              />
            </div>
          </div>
        </>
      )}
    </div>
  );
}
