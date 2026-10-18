/**
 * Terminal pane for the installation log.
 *
 * xterm.js rather than a `<pre>`: pacstrap and pacman emit ANSI — progress
 * bars that rewrite themselves in place, colour, cursor movement — and a plain
 * text node would render all of it as garbage escape sequences.
 *
 * The terminal is created once and written to imperatively through the handle,
 * because the installation emits thousands of lines and routing each one
 * through React state would re-render the tree on every line.
 */

import { useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal as XTerm } from "@xterm/xterm";

import "@xterm/xterm/css/xterm.css";

export interface TerminalHandle {
  write: (data: string) => void;
  clear: () => void;
}

interface TerminalProps {
  /** Receives the writer once the terminal is mounted. */
  handleRef: React.RefObject<TerminalHandle | null>;
  title?: string;
  className?: string;
}

export function Terminal({ handleRef, title, className }: TerminalProps) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    // Colours come from the design tokens so the terminal cannot drift from
    // the rest of the interface.
    const styles = getComputedStyle(document.documentElement);
    const token = (name: string, fallback: string) =>
      styles.getPropertyValue(name).trim() || fallback;

    const terminal = new XTerm({
      convertEol: true,
      cursorBlink: false,
      // The pane is read-only: the install log is output, not input. Typing is
      // handled by the rescue shell on another tty.
      disableStdin: true,
      fontFamily: token("--font-mono", "monospace"),
      fontSize: 12.5,
      lineHeight: 1.45,
      scrollback: 8000,
      theme: {
        background: "#06070a",
        foreground: token("--text-muted", "#9aa0a6"),
        cursor: token("--accent", "#8ab4f8"),
        selectionBackground: token("--accent-dim", "rgba(138,180,248,0.12)"),
        black: "#1a1d21",
        red: token("--danger", "#f28b82"),
        green: token("--success", "#81c995"),
        yellow: token("--warning", "#fdd663"),
        blue: token("--accent", "#8ab4f8"),
        magenta: "#c58af9",
        cyan: "#78d9ec",
        white: token("--text", "#e8eaed"),
      },
    });

    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(host);

    // The container has no size until layout has run, so fit once now and
    // again whenever it changes.
    const resize = () => {
      try {
        fit.fit();
      } catch {
        // fit() throws if the host is not laid out yet; the observer below
        // will call it again once it is.
      }
    };
    resize();

    const observer = new ResizeObserver(resize);
    observer.observe(host);

    handleRef.current = {
      write: (data: string) => terminal.write(data),
      clear: () => terminal.clear(),
    };

    return () => {
      observer.disconnect();
      handleRef.current = null;
      terminal.dispose();
    };
  }, [handleRef]);

  return (
    <div className={`term${className ? ` ${className}` : ""}`}>
      {title && (
        <div className="term__head">
          <span>{title}</span>
        </div>
      )}
      <div className="term__body" ref={hostRef} />
    </div>
  );
}
