import { useCallback, useEffect, useMemo, useState } from "react";

import { Badge, Button } from "./components/primitives";
import { ChevronLeft, ChevronRight, Power } from "./components/icons";
import { Logo } from "./components/Logo";
import { I18nProvider, useI18n } from "./i18n";
import {
  detectHardware,
  keyboardForCountries,
  listDisks,
  reboot,
  validateConfig,
  type Disk,
} from "./lib/api";
import { ZONES } from "./lib/worldData";
import {
  INITIAL_DRAFT,
  isAccountComplete,
  toInstallConfig,
  type Draft,
} from "./lib/draft";
import { useInstall } from "./lib/useInstall";

import { AccountScreen } from "./screens/Account";
import { DesktopScreen } from "./screens/Desktop";
import { DiskScreen } from "./screens/Disk";
import { InstallScreen } from "./screens/Install";
import { KeyboardScreen } from "./screens/Keyboard";
import { LanguageScreen } from "./screens/Language";
import { NetworkScreen } from "./screens/Network";
import { OptionsScreen } from "./screens/Options";
import { SummaryScreen } from "./screens/Summary";

import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/components.css";
import "./styles/screens.css";

/** The wizard's steps, in order. */
const STEPS = [
  "language",
  "keyboard",
  "network",
  "disk",
  "account",
  "desktop",
  "options",
  "summary",
  "install",
] as const;

type StepId = (typeof STEPS)[number];

function Shell() {
  const { t, locale, setLocale } = useI18n();

  const [stepIndex, setStepIndex] = useState(0);
  const [draft, setDraft] = useState<Draft>(INITIAL_DRAFT);
  const [diskConfirmed, setDiskConfirmed] = useState(false);
  // Set once the user picks a layout themselves. Until then, the keyboard
  // follows the time zone.
  const [keyboardChosen, setKeyboardChosen] = useState(false);
  const [disks, setDisks] = useState<Disk[]>([]);
  const [drivers, setDrivers] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const install = useInstall();
  const step: StepId = STEPS[stepIndex];

  // Hardware and disks are loaded once: they cannot change during a session,
  // and re-running lsblk on every step would be pointless work.
  useEffect(() => {
    listDisks().then(setDisks).catch(() => setDisks([]));
    detectHardware()
      .then((hardware) => setDrivers(hardware.codecPackages))
      .catch(() => setDrivers([]));
  }, []);

  const update = useCallback((patch: Partial<Draft>) => {
    setDraft((previous) => ({ ...previous, ...patch }));
    setError(null);
  }, []);

  // Selecting French should switch the interface, not just the installed
  // system's locale.
  useEffect(() => {
    setLocale(draft.locale);
  }, [draft.locale, setLocale]);

  // Preselect the keyboard from the time zone.
  //
  // `zone1970.tab` carries the ISO country codes each zone covers, and those
  // are already compiled into the bundle for the map — so a zone is enough to
  // guess the layout. It only ever *suggests*: once the user has picked a
  // layout on the keyboard screen, this stops.
  useEffect(() => {
    if (keyboardChosen) return;

    const countries = ZONES.find((zone) => zone.id === draft.timezone)?.countries;
    if (!countries) return;

    keyboardForCountries(countries.split(","))
      .then((keyboard) => {
        // Guard against a slower reply overwriting a choice made meanwhile.
        if (!keyboardChosen) {
          setDraft((previous) => ({ ...previous, keyboard: keyboard.id }));
        }
      })
      .catch(() => {
        // The engine decides the fallback; leaving the current value is fine.
      });
  }, [draft.timezone, keyboardChosen]);

  /** Whether the current step is complete enough to move on. */
  const canAdvance = useMemo(() => {
    switch (step) {
      case "disk":
        return Boolean(draft.disk) && diskConfirmed;
      case "account":
        return isAccountComplete(draft);
      default:
        return true;
    }
  }, [step, draft, diskConfirmed]);

  const advance = async () => {
    // Only the final step validates the whole configuration.
    //
    // Validating everything on every step was a bug: the first step was
    // rejected for an empty username the user had not been asked for yet. Each
    // step owns a subset of the fields, checked by `canAdvance`; the engine's
    // full check belongs where it is meaningful — immediately before anything
    // touches a disk.
    if (step === "summary") {
      const problem = await validateConfig(toInstallConfig(draft));
      if (problem) {
        setError(problem);
        return;
      }
      setError(null);
      setStepIndex(STEPS.indexOf("install"));
      install.start(toInstallConfig(draft));
      return;
    }

    setError(null);
    setStepIndex((index) => Math.min(index + 1, STEPS.length - 1));
  };

  const back = () => {
    setError(null);
    setStepIndex((index) => Math.max(index - 1, 0));
  };

  const finished = install.state.finished;

  return (
    <div className="app">
      <header className="app__topbar">
        <div className="app__brand">
          {/* The beam keeps moving while the wizard is idle, and stops once
           * the installation starts — at that point the progress bar is the
           * only thing that should be animating. */}
          <Logo height={24} animated={!install.state.running} />
          <span className="app__brand-tagline">{t("app.tagline")}</span>
        </div>

        {step === "language" ? null : (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => setLocale(locale === "fr" ? "en" : "fr")}
            style={{ minHeight: 32, padding: "0 var(--s-3)" }}
          >
            <Badge tone="accent">{locale === "fr" ? "EN" : "FR"}</Badge>
          </button>
        )}
      </header>

      <div className="app__body">
        <nav className="rail" aria-label={t("app.tagline")}>
          {STEPS.map((id, index) => (
            <div
              key={id}
              className={[
                "rail__step",
                index === stepIndex ? "rail__step--active" : "",
                index < stepIndex ? "rail__step--done" : "",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              <span className="rail__marker">
                {index < stepIndex ? "✓" : index + 1}
              </span>
              {t(`steps.${id}`)}
            </div>
          ))}
        </nav>

        <main className="app__content">
          <div
            className={`app__content-inner${step === "install" ? " app__content-inner--fill" : ""}`}
          >
            {step === "language" && <LanguageScreen draft={draft} update={update} />}
            {step === "keyboard" && (
              <KeyboardScreen
                draft={draft}
                update={(patch) => {
                  // Remember that the user chose deliberately, so the
                  // preselection from the time zone stops overwriting it.
                  if (patch.keyboard) setKeyboardChosen(true);
                  update(patch);
                }}
              />
            )}
            {step === "network" && <NetworkScreen />}
            {step === "disk" && (
              <DiskScreen
                draft={draft}
                update={update}
                disks={disks}
                confirmed={diskConfirmed}
                setConfirmed={setDiskConfirmed}
              />
            )}
            {step === "account" && <AccountScreen draft={draft} update={update} />}
            {step === "desktop" && <DesktopScreen draft={draft} update={update} />}
            {step === "options" && <OptionsScreen draft={draft} update={update} />}
            {step === "summary" && (
              <SummaryScreen draft={draft} disks={disks} drivers={drivers} />
            )}
            {step === "install" && <InstallScreen install={install} />}
          </div>
        </main>
      </div>

      <footer className="app__footer">
        {error && (
          <span className="app__error" role="alert">
            {error}
          </span>
        )}

        {step === "install" ? (
          <>
            <span className="app__footer-spacer" />
            {finished?.success && (
              <Button variant="primary" onClick={() => void reboot()}>
                <Power size={16} />
                {t("install.restart")}
              </Button>
            )}
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={back} disabled={stepIndex === 0}>
              <ChevronLeft size={16} />
              {t("common.back")}
            </Button>

            <span className="app__footer-spacer" />

            <Button variant="primary" onClick={() => void advance()} disabled={!canAdvance}>
              {step === "summary" ? t("common.install") : t("common.next")}
              <ChevronRight size={16} />
            </Button>
          </>
        )}
      </footer>
    </div>
  );
}

export default function App() {
  return (
    <I18nProvider>
      <Shell />
    </I18nProvider>
  );
}
