import { createRoot } from "react-dom/client";

import App from "./App";

/**
 * Entry point.
 *
 * Deliberately not wrapped in `StrictMode`. Its double-invocation of effects
 * exists to surface bugs in development, but this app's effects open real
 * resources — a terminal, a disk inventory — and the installer is a kiosk
 * that is never run in development mode on a live medium. The cost of the
 * double work is real; the benefit is not.
 */

const host = document.getElementById("root");
if (!host) {
  throw new Error("élément #root introuvable dans index.html");
}

createRoot(host).render(<App />);
