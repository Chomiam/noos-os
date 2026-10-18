/**
 * Password strength meter.
 *
 * The score comes from zxcvbn, evaluated in Rust — the dictionaries are
 * compiled into the binary, so nothing is fetched and the webview carries no
 * extra weight. Evaluation is debounced because zxcvbn is not free and the
 * user is typing.
 *
 * The score is advisory: it is shown on every keystroke, including bad ones.
 * Enforcement happens when the configuration is submitted.
 */

import { useEffect, useState } from "react";

import { useI18n } from "../i18n";
import { checkPassword, MIN_STRONG_SCORE, type PasswordStrength } from "../lib/api";

const DEBOUNCE_MS = 140;

/** Labels and colours, indexed by zxcvbn score. */
const TONE = [
  "var(--strength-0)",
  "var(--strength-1)",
  "var(--strength-2)",
  "var(--strength-3)",
  "var(--strength-4)",
] as const;

interface PasswordMeterProps {
  password: string;
  /** Username and full name: zxcvbn penalises passwords built from them. */
  userInputs: string[];
  /** When true, the meter also reports that the password is below the bar. */
  requireStrong: boolean;
}

export function PasswordMeter({
  password,
  userInputs,
  requireStrong,
}: PasswordMeterProps) {
  const { t } = useI18n();
  const [strength, setStrength] = useState<PasswordStrength | null>(null);

  // Serialised so the effect re-runs on content, not on array identity.
  const inputsKey = userInputs.join("\u0000");

  useEffect(() => {
    if (!password) {
      setStrength(null);
      return;
    }

    let cancelled = false;
    const timer = setTimeout(() => {
      checkPassword(password, inputsKey ? inputsKey.split("\u0000") : [])
        .then((result) => {
          if (!cancelled) setStrength(result);
        })
        .catch(() => {
          // A failed evaluation must not block the form; the hard check
          // happens server-side on submit anyway.
          if (!cancelled) setStrength(null);
        });
    }, DEBOUNCE_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [password, inputsKey]);

  if (!password) return null;

  const score = strength?.score ?? 0;
  const tooWeak = requireStrong && score < MIN_STRONG_SCORE;

  return (
    <div className="meter">
      <div className="meter__head">
        <span className="meter__label">{t("account.strengthLabel")}</span>
        <span className="meter__value" style={{ color: TONE[score] }}>
          {t(`account.strength.${score}`)}
        </span>
      </div>

      <div
        className="meter__bars"
        role="meter"
        aria-valuemin={0}
        aria-valuemax={4}
        aria-valuenow={score}
        aria-label={t("account.strengthLabel")}
      >
        {[0, 1, 2, 3, 4].map((index) => (
          <span key={index} className="meter__bar">
            <span
              className="meter__bar-fill"
              style={{
                width: index <= score ? "100%" : "0%",
                background: TONE[score],
              }}
            />
          </span>
        ))}
      </div>

      {tooWeak ? (
        <p className="meter__feedback" style={{ color: "var(--danger)" }}>
          {t("account.error.passwordWeak", {
            score,
            min: MIN_STRONG_SCORE,
          })}
        </p>
      ) : strength?.warning ? (
        <p className="meter__feedback">{strength.warning}</p>
      ) : strength?.suggestions.length ? (
        <p className="meter__feedback">{strength.suggestions[0]}</p>
      ) : null}
    </div>
  );
}
