import { Head } from "../components/primitives";
import { WorldMap } from "../components/WorldMap";
import { useI18n } from "../i18n";
import type { Draft } from "../lib/draft";

/**
 * Time zone.
 *
 * Clicking the map picks the nearest reference city; the search box is there
 * because a click can never be precise about which of two adjacent zones the
 * user meant, and because keyboard-only use has to be possible.
 */
export function TimezoneScreen({
  draft,
  update,
}: {
  draft: Draft;
  update: (patch: Partial<Draft>) => void;
}) {
  const { t } = useI18n();

  return (
    <div className="fade-in">
      <Head title={t("timezone.title")} subtitle={t("timezone.subtitle")} />

      <WorldMap
        selected={draft.timezone}
        onSelect={(timezone) => update({ timezone })}
      />
    </div>
  );
}
