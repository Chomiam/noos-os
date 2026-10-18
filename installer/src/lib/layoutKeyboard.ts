/**
 * Keystroke remapping, so the chosen layout applies without touching the
 * compositor.
 *
 * `cage` reads the keyboard layout from the environment once at startup and no
 * Wayland protocol lets a client change its compositor's keymap. Restarting the
 * compositor to apply a layout showed the user a visible round-trip through the
 * tty.
 *
 * The installer is the only thing the user types into, so it translates
 * keystrokes itself. `KeyboardEvent.code` names the *physical* key and is
 * layout-independent; the generated tables say what character that key
 * produces under each layout.
 */

import { useCallback, useEffect, useRef } from "react";

import { KEYMAPS, type Levels } from "./keymaps";

/** The levels array for a physical key, or undefined if it types nothing. */
function levelsFor(layout: string, code: string): Levels | undefined {
  return KEYMAPS[layout]?.[code];
}

/**
 * Which of the four levels a keystroke selects.
 *
 * AltGr is tracked from the physical `AltRight` key rather than from
 * `getModifierState("AltGraph")`. That method reports the *compositor's*
 * modifier state, and the compositor is running whatever layout it started
 * with — so on a US session, pressing AltGr on a French layout reports nothing
 * at all. `event.code` does not have that problem: it names the key, not the
 * character.
 */
export interface ModifierState {
  shift: boolean;
  altGr: boolean;
  capsLock: boolean;
}

export function levelIndex(state: ModifierState): 0 | 1 | 2 | 3 {
  if (state.shift) return state.altGr ? 3 : 1;
  return state.altGr ? 2 : 0;
}

/**
 * Applies Caps Lock to a character.
 *
 * XKB treats Caps Lock as a shift-toggle for alphabetic keys only, so `2` stays
 * `2` while `a` becomes `A`. Only single letters are touched.
 */
export function applyCapsLock(char: string, capsLock: boolean): string {
  if (!capsLock || char.length !== 1) return char;
  const lower = char.toLowerCase();
  const upper = char.toUpperCase();
  if (lower === upper) return char;
  return char === lower ? upper : lower;
}

/** The character a physical key produces, or null if it produces none. */
export function characterFor(
  layout: string,
  code: string,
  state: ModifierState,
): string | null {
  const levels = levelsFor(layout, code);
  if (!levels) return null;

  const char = levels[levelIndex(state)];
  // An empty level means the key types nothing there — a dead key, or a level
  // the layout does not define.
  if (!char) return null;

  return applyCapsLock(char, state.capsLock);
}

/**
 * Tracks AltGr from the physical key, and returns a handler pair to attach to
 * the same element as the remapping.
 */
export function useAltGr() {
  const held = useRef(false);

  const onKeyDown = useCallback((event: React.KeyboardEvent) => {
    if (event.code === "AltRight") held.current = true;
  }, []);

  const onKeyUp = useCallback((event: React.KeyboardEvent) => {
    if (event.code === "AltRight") held.current = false;
  }, []);

  // A window blur while AltGr is held would leave the flag stuck on.
  useEffect(() => {
    const release = () => {
      held.current = false;
    };
    window.addEventListener("blur", release);
    return () => window.removeEventListener("blur", release);
  }, []);

  return { held, onKeyDown, onKeyUp };
}

/**
 * Remaps a keystroke into the character the chosen layout would produce.
 *
 * Returns the character to insert, or `null` to let the browser handle the
 * event normally.
 *
 * Two cases deliberately fall through to the browser:
 *
 *  * **Ctrl or Meta is held.** Those are shortcuts — copy, paste, select-all —
 *    and remapping them would break every one.
 *  * **Alt is held without AltGr.** Window-manager and application shortcuts.
 *
 * A key the table does not know — arrows, Backspace, Tab, Enter — also falls
 * through, which is what makes editing work at all.
 */
export function remap(
  event: React.KeyboardEvent,
  layout: string,
  altGrHeld: boolean,
): string | null {
  if (event.ctrlKey || event.metaKey) return null;
  // Alt is AltGr only when the *right* Alt key is down.
  if (event.altKey && !altGrHeld) return null;

  return characterFor(layout, event.code, {
    shift: event.shiftKey,
    altGr: altGrHeld,
    capsLock: event.getModifierState("CapsLock"),
  });
}

/**
 * Inserts a character at the caret, replacing any selection.
 *
 * Reads the caret from the live DOM element rather than from React state: the
 * selection is not part of the controlled value, and by the time a `keydown`
 * handler runs React has not re-rendered.
 */
export function insertAtCaret(
  element: HTMLInputElement,
  char: string,
): { value: string; caret: number } {
  const start = element.selectionStart ?? element.value.length;
  const end = element.selectionEnd ?? start;

  const value = element.value.slice(0, start) + char + element.value.slice(end);
  return { value, caret: start + char.length };
}
