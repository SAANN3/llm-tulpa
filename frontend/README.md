# Frontend
React + TypeScript UI for llm-tulpa. Talks to the backend over its REST API, nothing else.

## Requirements
- Node 20+ (tested with 23)

## Setup
```bash
npm install
cp .env.example .env
```
Bun works too (`bun install`). Both `package-lock.json` and `bun.lock` are tracked so either tool installs reproducibly — the catch is that a dependency change has to be applied with both (`npm install` and `bun install`) or the two drift apart.
`.env`'s `VITE_BACKEND_URL` only needs setting if the backend runs somewhere other than the host this page itself was loaded from — left empty, it's derived at runtime from `window.location.hostname` on port 3000, which is right for the common case (backend and frontend served from the same machine, whether that's `localhost` or a LAN IP).

## Running
```bash
npm run dev
```
Or build for production:
```bash
npm run build
npm run preview
```
Or via Docker — see the repo root's `compose.yaml`.

### The container: http, https and the certificate
The nginx in the image serves the page on port 80 (published as `5173`) and, for the things a browser only allows on a secure page (the microphone), on 443 (published as `5443`, `FRONTEND_HTTPS_PORT`). Both servers forward `/api` to the backend (one file, `api-proxy.inc.template`, included by each). The https one needs it, because an https page may not call an http address: `config.ts` makes the backend the page's own origin there. The plain one does not use it (a page on http calls the backend on port 3000 directly); it is there for a reverse proxy in front of the container, which can then send everything to the one port (see the root README, "Behind your own domain and reverse proxy"). `nginx.conf.template` is filled in at start from the environment: `BACKEND_ADDR` (default `host.docker.internal:3000`, the backend on the host or on its network).

`docker-entrypoint.d/40-tls.sh` keeps the self-signed certificate. It makes one when the container starts and none exists, and again when it is missing (delete `generated/` and it is back within seconds), when the names it should cover differ from the ones it was made for, a month before it ends (it lasts 825 days), and when asked. The names are `TLS_NAMES` (the start scripts pass this machine's addresses as they are when they run, so a restart after the machine's address changed makes a new one) and the lines of `/tls/extra-names.txt`; Settings → Voice → "Make a new one" has the backend write that file with the machine's addresses as they are now and the address the owner is using, then create `/tls/regenerate`, which the container takes away once it has made the certificate. A certificate the owner uploads in Settings → Voice is written by the backend into the `custom/` folder of the same mounted directory (`TLS_DIR`, default `~/.llm-tulpa/tls`, which is also the backend's default `tls_dir`) and served instead while both files are there; a loop in the script looks at those files every five seconds with a single `stat` (about 0.04% of a core), does the checks and reloads nginx only when something changed (and checks the expiry once an hour), and ignores a pair that does not belong together.

## Structure
Files and folders are kebab-case (`chat-entry.tsx`, `use-messages.ts`, `tool-confirmation.tsx`); a component's name inside the file is PascalCase, a hook's is `useX`.

```
src/
├── api/             # thin per-route HTTP clients, mirrors the backend's routes 1:1
├── components/      # composed UI pieces, built from primitives/
│   └── primitives/  # the themed building blocks — one fixed implementation per element, styled per-theme via CSS alone
├── context/         # app-wide React context providers
├── hooks/           # data-fetching and other stateful logic shared across pages
├── pages/           # one file per route (the setup wizard is a folder: a shell plus one file per step)
├── styles/          # one .scss per component/page, plus variants.scss (the shared variant system)
└── themes/          # one .scss per theme (the three palette colors) — see THEMING.md
```

## Docs
- [THEMING.md](./THEMING.md) — how theming works: the primitive/composed-component split, and the rules around `variant`/`data-theme` CSS that keep themes from colliding.
