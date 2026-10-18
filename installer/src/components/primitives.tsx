/**
 * The design system primitives.
 *
 * Kept together in one file because they are used everywhere and share a
 * visual vocabulary; splitting them across files would make it easy for one
 * to drift.
 */

import type { ReactNode, InputHTMLAttributes } from "react";

import { Alert, Check, Info } from "./icons";
import { insertAtCaret, remap, useAltGr } from "../lib/layoutKeyboard";

/* ============================================================== heading === */

export function Head({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <header className="head">
      <h1 className="head__title">{title}</h1>
      {subtitle && <p className="head__subtitle">{subtitle}</p>}
    </header>
  );
}

/* =============================================================== button === */

type ButtonVariant = "primary" | "ghost" | "outline" | "danger";

interface ButtonProps {
  children: ReactNode;
  onClick?: () => void;
  variant?: ButtonVariant;
  disabled?: boolean;
  type?: "button" | "submit";
  autoFocus?: boolean;
}

export function Button({
  children,
  onClick,
  variant = "ghost",
  disabled = false,
  type = "button",
  autoFocus = false,
}: ButtonProps) {
  return (
    <button
      type={type}
      className={`btn btn--${variant}`}
      onClick={onClick}
      disabled={disabled}
      autoFocus={autoFocus}
    >
      {children}
    </button>
  );
}

/* ================================================================ field === */

interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  hint?: string;
  error?: string;
  optional?: boolean;
  /**
   * Keyboard layout to remap keystrokes into.
   *
   * The compositor's own layout cannot be changed at runtime, so the installer
   * translates keystrokes itself — see `lib/layoutKeyboard`. Every text field
   * takes this, or the user types their name with the wrong layout.
   */
  layout: string;
}

export function Field({
  label,
  hint,
  error,
  optional = false,
  layout,
  id,
  onChange,
  ...input
}: FieldProps) {
  const inputId = id ?? `field-${label.replace(/\s+/g, "-").toLowerCase()}`;
  // The error is referenced by the input so screen readers announce it with
  // the field rather than as loose text.
  const describedBy = error ? `${inputId}-error` : hint ? `${inputId}-hint` : undefined;

  const { held, onKeyDown: trackAltGrDown, onKeyUp: trackAltGrUp } = useAltGr();

  /**
   * Translates the keystroke before the browser sees it.
   *
   * `preventDefault` is what stops the compositor's own layout from also
   * inserting a character: without it the field would receive both.
   */
  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    trackAltGrDown(event);
    if (event.defaultPrevented) return;

    const char = remap(event, layout, held.current);
    if (char === null) return;

    event.preventDefault();

    const target = event.currentTarget;
    const { value, caret } = insertAtCaret(target, char);

    // Report through the same channel as ordinary typing, so the caller does
    // not need to know the value was synthesised.
    if (onChange) {
      const synthetic = {
        ...event,
        target: { ...target, value },
        currentTarget: { ...target, value },
      } as unknown as React.ChangeEvent<HTMLInputElement>;
      onChange(synthetic);
    }

    // React has not re-rendered yet, so the caret is placed on the next frame.
    requestAnimationFrame(() => {
      target.setSelectionRange(caret, caret);
    });
  };

  return (
    <div className="field">
      <label className="field__label" htmlFor={inputId}>
        {label}
        {optional && <span className="field__optional"> — facultatif</span>}
      </label>

      <input
        {...input}
        id={inputId}
        onChange={onChange}
        onKeyDown={handleKeyDown}
        onKeyUp={trackAltGrUp}
        className={`field__input${error ? " field__input--invalid" : ""}`}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        spellCheck={false}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
      />

      {error ? (
        <p className="field__error" id={`${inputId}-error`}>
          {error}
        </p>
      ) : hint ? (
        <p className="field__hint" id={`${inputId}-hint`}>
          {hint}
        </p>
      ) : null}
    </div>
  );
}

/* =============================================================== switch === */

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  title: string;
  hint?: string;
  disabled?: boolean;
}

export function Switch({
  checked,
  onChange,
  title,
  hint,
  disabled = false,
}: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      className={`switch${checked ? " switch--on" : ""}${disabled ? " switch--disabled" : ""}`}
      onClick={() => !disabled && onChange(!checked)}
    >
      <span className="switch__track">
        <span className="switch__thumb" />
      </span>
      <span className="switch__text">
        <span className="switch__title">{title}</span>
        {hint && <span className="switch__hint">{hint}</span>}
      </span>
    </button>
  );
}

/* ================================================================= card === */

type CardTone = "plain" | "warning" | "danger" | "success";

export function Card({
  tone = "plain",
  title,
  children,
}: {
  tone?: CardTone;
  title?: string;
  children: ReactNode;
}) {
  const icon =
    tone === "warning" || tone === "danger" ? (
      <Alert size={18} />
    ) : tone === "success" ? (
      <Check size={18} />
    ) : (
      <Info size={18} />
    );

  return (
    <div className={`card${tone === "plain" ? "" : ` card--${tone}`}`}>
      {title && (
        <div className="card__title">
          {icon}
          {title}
        </div>
      )}
      <div className="card__body">{children}</div>
    </div>
  );
}

/* ================================================================ badge === */

type BadgeTone = "neutral" | "accent" | "success" | "warning" | "danger";

export function Badge({
  tone = "neutral",
  children,
}: {
  tone?: BadgeTone;
  children: ReactNode;
}) {
  return (
    <span className={`badge${tone === "neutral" ? "" : ` badge--${tone}`}`}>
      {children}
    </span>
  );
}

/* =============================================================== choice === */

export function Choice({
  selected,
  onSelect,
  title,
  description,
}: {
  selected: boolean;
  onSelect: () => void;
  title: string;
  description?: string;
}) {
  return (
    <button
      type="button"
      className={`choice${selected ? " choice--selected" : ""}`}
      onClick={onSelect}
      aria-pressed={selected}
    >
      <span className="choice__title">{title}</span>
      {description && <span className="choice__desc">{description}</span>}
      <span className="choice__check">
        <Check size={13} />
      </span>
    </button>
  );
}
