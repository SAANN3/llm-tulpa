// No trailing slash, no `/api`. `VITE_BACKEND_URL` (baked in at build time) overrides this when
// the backend isn't on the host the page was loaded from; unset by default so the same build
// works from `localhost` or a LAN IP — the hostname the browser used is where the backend is.
export const BACKEND_URL =
    import.meta.env.VITE_BACKEND_URL ||
    `http://${window.location.hostname}:3000`
