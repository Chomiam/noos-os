import { Card } from "../components/primitives";
import { Orb } from "../components/Orb";
import { Terminal } from "../components/Terminal";
import { useI18n } from "../i18n";
import type { useInstall } from "../lib/useInstall";

/**
 * Installation progress.
 *
 * The terminal is the real thing: pacstrap's own output, ANSI and all, through
 * xterm.js. Showing a scrubbed "prettified" log would hide exactly the
 * information needed when something goes wrong.
 */
export function InstallScreen({
  install,
}: {
  install: ReturnType<typeof useInstall>;
}) {
  const { t } = useI18n();
  const { state, logRef } = install;

  const done = state.finished !== null;
  const failed = state.finished !== null && !state.finished.success;

  return (
    <div className="install fade-in">
      <div className="progress">
        <div className="progress__head">
          <Orb size={40} active={state.running} />

          <div className="progress__text">
            {state.stepTotal > 0 && (
              <div className="progress__step">
                {t("install.step", {
                  index: state.stepIndex,
                  total: state.stepTotal,
                })}
              </div>
            )}
            <div className="progress__message">
              {done
                ? state.finished?.success
                  ? t("install.doneTitle")
                  : t("install.failedTitle")
                : state.stepTitle || t("install.title")}
            </div>
          </div>

          {state.running && (
            <span className="disk__size">{Math.round(state.percent * 100)} %</span>
          )}
        </div>

        <div className="progress__track">
          <div
            className="progress__fill"
            style={{
              width: `${Math.round(state.percent * 100)}%`,
              background: failed ? "var(--danger)" : undefined,
            }}
          />
        </div>
      </div>

      {done && (
        <div style={{ marginTop: "var(--s-4)" }}>
          <Card tone={failed ? "danger" : "success"}>
            {failed ? (
              <>
                <strong>{state.finished?.message}</strong>
                <p style={{ marginTop: "var(--s-2)" }}>{t("install.failedBody")}</p>
              </>
            ) : (
              t("install.doneBody")
            )}
          </Card>
        </div>
      )}

      <Terminal
        handleRef={logRef}
        title={t("install.log")}
        className="install__term"
      />
    </div>
  );
}
