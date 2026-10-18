import { useEffect, useMemo, useState } from "react";

import { Head } from "../components/primitives";
import { Search } from "../components/icons";
import { useI18n } from "../i18n";
import { listKeyboards, type Keyboard } from "../lib/api";
import { insertAtCaret, remap, useAltGr } from "../lib/layoutKeyboard";
import type { Draft } from "../lib/draft";

/**
 * Keyboard layout.
 *
 * Two things make this screen work:
 *
 *  - a **test field**, so the layout can be verified by typing rather than
 *    guessed at. Someone who cannot find their `@` has no way to tell whether
 *    the installer picked right, and every later screen needs a working
 *    keyboard.
 *  - the layout list **in its own language** — "Deutsch", not "German".
 *
 * The test field remaps keystrokes through the same code path the rest of the
 * installer uses, so what it shows is exactly what the name and password
 * fields will receive. Choosing a layout applies immediately: nothing has to
 * restart.
 *
 * The preselection from the time zone happens in the wizard, not here, so that
 * coming back to this screen does not overwrite a deliberate choice.
 */
export function KeyboardScreen({
  draft,
  update,
}: {
  draft: Draft;
  update: (patch: Partial<Draft>) => void;
}) {
  const { t } = useI18n();
  const [keyboards, setKeyboards] = useState<Keyboard[]>([]);
  const [query, setQuery] = useState("");
  const [test, setTest] = useState("");

  const { held, onKeyDown: trackAltGrDown, onKeyUp: trackAltGrUp } = useAltGr();

  useEffect(() => {
    listKeyboards().then(setKeyboards).catch(() => setKeyboards([]));
  }, []);

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return keyboards;
    return keyboards.filter(
      (keyboard) =>
        keyboard.label.toLowerCase().includes(needle) ||
        keyboard.id.toLowerCase().includes(needle) ||
        keyboard.x11.toLowerCase().includes(needle),
    );
  }, [keyboards, query]);

  const selected = keyboards.find((keyboard) => keyboard.id === draft.keyboard);

  const handleTestKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    trackAltGrDown(event);
    if (event.defaultPrevented) return;

    const char = remap(event, draft.keyboard, held.current);
    if (char === null) return;

    event.preventDefault();
    const { value, caret } = insertAtCaret(event.currentTarget, char);
    setTest(value);
    requestAnimationFrame(() => event.currentTarget.setSelectionRange(caret, caret));
  };

  return (
    <div className="fade-in">
      <Head title={t("keyboard.title")} subtitle={t("keyboard.subtitle")} />

      {/* The test field comes first, before the choice: the reason to look at
       * this screen at all is to check what the keyboard produces. */}
      <div className="kb-test">
        <label className="kb-test__label" htmlFor="kb-test-input">
          {t("keyboard.test")}
        </label>
        <input
          id="kb-test-input"
          className="kb-test__input"
          value={test}
          onChange={(event) => setTest(event.target.value)}
          onKeyDown={handleTestKeyDown}
          onKeyUp={trackAltGrUp}
          placeholder={t("keyboard.testPlaceholder")}
          spellCheck={false}
          autoComplete="off"
        />
        <p className="kb-test__hint">
          {selected
            ? t("keyboard.current", { label: selected.label })
            : t("keyboard.none")}
        </p>
        <p className="kb-test__hint">{t("keyboard.appliedHint")}</p>
      </div>

      <div className="map__search" style={{ marginBottom: "var(--s-4)" }}>
        <Search size={16} />
        <input
          className="map__search-input"
          value={query}
          placeholder={t("keyboard.search")}
          onChange={(event) => setQuery(event.target.value)}
          spellCheck={false}
          autoComplete="off"
        />
      </div>

      {matches.length === 0 ? (
        <p className="dim">{t("timezone.noResult")}</p>
      ) : (
        <div className="kb-grid">
          {matches.map((keyboard) => (
            <button
              key={keyboard.id}
              type="button"
              className={`kb-item${draft.keyboard === keyboard.id ? " kb-item--on" : ""}`}
              onClick={() => update({ keyboard: keyboard.id })}
              aria-pressed={draft.keyboard === keyboard.id}
            >
              <span className="kb-item__label">{keyboard.label}</span>
              <span className="kb-item__code">{keyboard.id}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
