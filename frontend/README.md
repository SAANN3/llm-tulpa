# Frontend
React + TypeScript UI for llm-tulpa. Talks to the backend over its REST API, nothing else.

## Requirements
- Node 20+ (tested with 23)

## Setup
```bash
npm install
cp .env.example .env
```
`.env`'s `VITE_BACKEND_URL` should point at wherever the backend's running (`http://localhost:3000` by default).

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
```
src/
├── api/             # thin per-route HTTP clients, mirrors the backend's routes 1:1
├── components/      # composed UI pieces, built from primitives/
│   └── primitives/  # the themed building blocks — one fixed implementation per element, styled per-theme via CSS alone
├── context/         # app-wide React context providers
├── hooks/           # data-fetching and other stateful logic shared across pages
├── pages/           # one file per route
└── themes/          # theme CSS files + the variant system — see THEMING.md
```

## Docs
- [THEMING.md](./THEMING.md) — how theming works: the primitive/composed-component split, and the rules around `variant`/`data-theme` CSS that keep themes from colliding.
