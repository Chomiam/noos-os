import { Head, Choice } from "../components/primitives";
import { useI18n } from "../i18n";
import type { Desktop } from "../lib/api";
import type { Draft } from "../lib/draft";

/**
 * Desktop environment.
 *
 * The four options are mutually exclusive and the descriptions say what
 * actually distinguishes them, rather than repeating marketing copy. GNOME and
 * Budgie are offered side by side even though they share a lot of the GNOME
 * stack — that is the user's call, not ours.
 */
export function DesktopScreen({
  draft,
  update,
}: {
  draft: Draft;
  update: (patch: Partial<Draft>) => void;
}) {
  const { t } = useI18n();

  const options: Desktop[] = ["gnome", "plasma", "cinnamon", "budgie"];

  return (
    <div className="fade-in">
      <Head title={t("desktop.title")} subtitle={t("desktop.subtitle")} />

      <div className="choices choices--two">
        {options.map((desktop) => (
          <Choice
            key={desktop}
            selected={draft.desktop === desktop}
            onSelect={() => update({ desktop })}
            title={t(`desktop.${desktop}`)}
            description={t(`desktop.${desktop}Desc`)}
          />
        ))}
      </div>
    </div>
  );
}
