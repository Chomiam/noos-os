/**
 * Installation driver.
 *
 * Owns the install state so the screen and the footer can both read it, and
 * writes log lines straight to the terminal rather than through React state —
 * pacstrap emits thousands of them, and a re-render per line would make the
 * whole interface stutter during the one moment it must stay responsive.
 */

import { useCallback, useRef, useState } from "react";

import type { TerminalHandle } from "../components/Terminal";
import { startInstall, type InstallConfig } from "./api";

export interface InstallState {
  running: boolean;
  /**
   * Overall completion, 0..1.
   *
   * Derived from the step index rather than from what the steps report: only a
   * few of them can estimate their own progress, so a bar driven by those sat
   * at zero for minutes at a time — the whole of `pacstrap`. The step counter
   * is real, monotonic and always moving, which is what a progress bar is for.
   */
  percent: number;
  stepIndex: number;
  stepTotal: number;
  stepTitle: string;
  finished: { success: boolean; message: string } | null;
}

const INITIAL: InstallState = {
  running: false,
  percent: 0,
  stepIndex: 0,
  stepTotal: 0,
  stepTitle: "",
  finished: null,
};

/**
 * Matches pacman's package counter: `(12/345) installing foo`.
 *
 * Used to give the longest step a real percentage. It is a heuristic on
 * another program's output — if the format ever changes the match simply stops
 * and the bar falls back to step-level granularity, which is still correct.
 */
const PACMAN_COUNTER = /\(\s*(\d+)\/(\d+)\)/;

/** Frames the log so the steps stand out from the tool output. */
const RULE = "─".repeat(64);

export function useInstall() {
  const [state, setState] = useState<InstallState>(INITIAL);
  const logRef = useRef<TerminalHandle | null>(null);
  // Guards against a double start: the engine wipes a disk, and running it
  // twice concurrently would be a catastrophe rather than a bug.
  const startedRef = useRef(false);

  /** Writes a narration line, visually distinct from tool output. */
  const narrate = useCallback((text: string) => {
    logRef.current?.write(`\r\n\x1b[38;2;138;180;248m${text}\x1b[0m\r\n`);
  }, []);

  const start = useCallback(
    (config: InstallConfig) => {
      if (startedRef.current) return;
      startedRef.current = true;

      logRef.current?.clear();
      setState({ ...INITIAL, running: true });

      startInstall(config, (event) => {
        switch (event.event) {
          case "step": {
            const { index, total, title } = event.data;

            // Announce the step in the log. Without this the terminal is an
            // undifferentiated stream of tool output, and someone watching it
            // cannot tell a slow step from a stuck one.
            narrate(`${RULE}\r\n  ${index}/${total}  ${title}\r\n${RULE}`);

            setState((previous) => ({
              ...previous,
              stepIndex: index,
              stepTotal: total,
              stepTitle: title,
              percent: (index - 1) / total,
            }));
            break;
          }

          case "progress":
            setState((previous) => {
              const withinStep = event.data.percent ?? 0;
              return {
                ...previous,
                // Overall progress: completed steps plus the fraction of the
                // current one. Always moving, because the step index is.
                percent:
                  previous.stepTotal > 0
                    ? (previous.stepIndex - 1 + withinStep) / previous.stepTotal
                    : 0,
                stepTitle: event.data.message || previous.stepTitle,
              };
            });
            break;

          case "log": {
            // Straight to the terminal, never through state.
            logRef.current?.write(`${event.data.line}\r\n`);

            // Give the longest step a real percentage by reading pacman's own
            // package counter out of its output.
            const counter = PACMAN_COUNTER.exec(event.data.line);
            if (counter) {
              const done = Number(counter[1]);
              const total = Number(counter[2]);
              if (total > 0 && done <= total) {
                setState((previous) =>
                  previous.stepTotal > 0
                    ? {
                        ...previous,
                        percent:
                          (previous.stepIndex - 1 + done / total) /
                          previous.stepTotal,
                      }
                    : previous,
                );
              }
            }
            break;
          }

          case "finished":
            setState((previous) => ({
              ...previous,
              running: false,
              percent: event.data.success ? 1 : previous.percent,
              finished: event.data,
            }));

            if (event.data.success) {
              narrate("Installation terminée.");
            } else {
              narrate(`Échec : ${event.data.message.split("\n")[0]}`);
            }
            break;
        }
      }).catch((reason) => {
        setState((previous) => ({
          ...previous,
          running: false,
          finished: { success: false, message: String(reason) },
        }));
      });
    },
    [narrate],
  );

  return { state, start, logRef };
}
