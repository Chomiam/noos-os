import { Head } from "../components/primitives";
import { FlagFrance, FlagUnitedKingdom } from "../components/Flag";
import { WorldMap } from "../components/WorldMap";
import { useI18n, type Locale } from "../i18n";
import type { Draft } from "../lib/draft";

/**
 * First screen: language and time zone together.
 *
 * They are one step because they answer the same question — where is this
 * machine, and who is it for. Splitting them meant a whole screen for two
 * clicks, and a step indicator that promised more work than there was.
 *
 * The language names are shown in their own language rather than translated:
 * someone who has landed in the wrong locale needs to recognise their own.
 */
export function LanguageScreen({
  draft,
  update,
}: {
  draft: Draft;
  update: (patch: Partial<Draft>) => void;
}) {
  const { t } = useI18n();

  const languages = [
    { locale: "fr" as Locale, name: "Français", Flag: FlagFrance },
    { locale: "en" as Locale, name: "English", Flag: FlagUnitedKingdom },
  ];

  return (
    <div className="fade-in">
      <Head title={t("language.title")} subtitle={t("language.subtitle")} />

      <div className="lang-row" role="radiogroup" aria-label={t("steps.language")}>
        {languages.map(({ locale, name, Flag }) => (
          <button
            key={locale}
            type="button"
            role="radio"
            aria-checked={draft.locale === locale}
            className={`lang-pill${draft.locale === locale ? " lang-pill--on" : ""}`}
            onClick={() => update({ locale })}
          >
            <Flag height={20} />
            {name}
          </button>
        ))}
      </div>

      <div className="lang-divider">
        <span>{t("timezone.title")}</span>
      </div>

      <WorldMap
        selected={draft.timezone}
        onSelect={(timezone) => update({ timezone })}
      />
    </div>
  );
}
