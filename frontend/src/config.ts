// No trailing slash, no `/api`. `VITE_BACKEND_URL` (baked in at build time) overrides this when
// the backend isn't on the host the page was loaded from; unset by default so the same build
// works from `localhost` or a LAN IP — the hostname the browser used is where the backend is.
//
// A page served over https (the frontend container's secure port, which voice input needs) may not call an http
// address, so there the backend is the page's own origin, which the container forwards `/api` from; an override that
// is itself https is still taken.
const override = import.meta.env.VITE_BACKEND_URL
export const BACKEND_URL =
    window.location.protocol === 'https:'
        ? override?.startsWith('https:') ? override : ''
        : override || `http://${window.location.hostname}:3000`
