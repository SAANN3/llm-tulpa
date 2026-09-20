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

## Structure
Files and folders are kebab-case (`chat-entry.tsx`, `use-messages.ts`, `allow-scope.ts`); a component's name inside the file is PascalCase, a hook's is `useX`.

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
