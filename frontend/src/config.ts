/** Base URL of the backend, e.g. `http://localhost:3000` — no trailing slash, no `/api`.
 * `VITE_BACKEND_URL` (baked in at build time) overrides this when the backend isn't
 * reachable on the same host the page itself was loaded from — unset by default, so
 * the same build works from `localhost`, a LAN IP, or anything else: whichever
 * hostname the browser actually used to load this page is where the backend is too. */
export const BACKEND_URL =
  import.meta.env.VITE_BACKEND_URL || `http://${window.location.hostname}:3000`
