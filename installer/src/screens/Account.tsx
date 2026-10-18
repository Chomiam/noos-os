import { useState } from "react";

import { Field, Head, Switch } from "../components/primitives";
import { PasswordMeter } from "../components/PasswordMeter";
import { useI18n } from "../i18n";
import {
  validateFullName,
  validateHostname,
  validateUsername,
  type Draft,
  type FieldError,
} from "../lib/draft";

/**
 * Account creation.
 *
 * Validation is shown as the user types, but only once a field has content or
 * has been left — flagging an empty required field before it has been filled
 * in is just noise.
 */
export function AccountScreen({
  draft,
  update,
}: {
  draft: Draft;
  update: (patch: Partial<Draft>) => void;
}) {
  const { t } = useI18n();
  const [touched, setTouched] = useState<Record<string, boolean>>({});

  const markTouched = (field: string) =>
    setTouched((previous) => ({ ...previous, [field]: true }));

  /** Renders an error only when showing it would be useful. */
  const message = (field: string, error: FieldError | null): string | undefined => {
    if (!error) return undefined;
    const value = (draft as unknown as Record<string, string>)[field] ?? "";
    if (!touched[field] && !value) return undefined;
    return t(error.key, error.vars);
  };

  const passwordsMatch = !draft.passwordConfirm || draft.password === draft.passwordConfirm;

  return (
    <div className="fade-in">
      <Head title={t("account.title")} subtitle={t("account.subtitle")} />

      <Field
        layout={draft.keyboard}
        label={t("account.fullName")}
        hint={t("account.fullNameHint")}
        error={message("fullName", validateFullName(draft.fullName))}
        value={draft.fullName}
        onChange={(event) => update({ fullName: event.target.value })}
        onBlur={() => markTouched("fullName")}
      />

      <Field
        layout={draft.keyboard}
        label={t("account.username")}
        hint={t("account.usernameHint")}
        error={message("username", validateUsername(draft.username))}
        value={draft.username}
        onChange={(event) => update({ username: event.target.value })}
        onBlur={() => markTouched("username")}
      />

      <Field
        layout={draft.keyboard}
        label={t("account.hostname")}
        hint={t("account.hostnameHint")}
        error={message("hostname", validateHostname(draft.hostname))}
        value={draft.hostname}
        onChange={(event) => update({ hostname: event.target.value })}
        onBlur={() => markTouched("hostname")}
      />

      <Field
        layout={draft.keyboard}
        label={t("account.password")}
        type="password"
        value={draft.password}
        onChange={(event) => update({ password: event.target.value })}
      />

      <PasswordMeter
        password={draft.password}
        userInputs={[draft.username, draft.fullName]}
        requireStrong={draft.requireStrongPassword}
      />

      <Field
        layout={draft.keyboard}
        label={t("account.passwordConfirm")}
        type="password"
        error={!passwordsMatch ? t("account.mismatch") : undefined}
        value={draft.passwordConfirm}
        onChange={(event) => update({ passwordConfirm: event.target.value })}
      />

      <Switch
        checked={draft.requireStrongPassword}
        onChange={(requireStrongPassword) => update({ requireStrongPassword })}
        title={t("account.requireStrong")}
        hint={t("account.requireStrongHint")}
      />
    </div>
  );
}
