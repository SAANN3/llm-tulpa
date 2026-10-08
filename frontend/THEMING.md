# Theming

## Quickstart: adding a theme
1. Create `src/themes/<name>.scss`:
   ```scss
   [data-theme="<name>"] {
     --color-primary: #...;
     --color-secondary: #...;
     --color-tertiary: #...;
   }
   ```
2. In `src/themes/index.ts`, import that file and add `<name>` to `themeNames`:
   ```ts
   import './<name>.scss'
   // ...
   export const themeNames = [/* existing names */, '<name>'] as const
   ```

A theme fills in exactly those three colors. Everything else a component needs is derived
from them in `styles/variants.scss` (`--color-surface`, `--color-border`, ...) or is a
semantic value that's the same on every theme (`--color-danger`) — defined once there, never
per theme, and never as a literal color inside a component's own `.scss`.

That's it — nothing in `primitives/` or any composed component ever needs to change; every
UI piece already re-colors itself off those three variables. Read on for why it's built
this way and the rules that keep multiple themes from colliding.

---

How theming is laid out across `src/`, and — the part that isn't obvious from reading any
single file — the rules around `variant`/`data-theme` CSS that keep multiple themes from
silently colliding with each other. Read this before adding a new theme or touching
`styles/variants.scss`.

- `components/primitives/` — the fixed component set, see below. Stays under
  `components/` since it's the one piece here that's actually made of React components.
- `src/context/` — `theme-context.ts`/`theme-provider.tsx`/`use-theme.ts`, split into three
  files (not because any of them is complex) because oxlint's `only-export-components`
  fast-refresh rule flags any file that exports a component alongside a non-component
  value — the context object and the hook both count, so they can't share a file with the
  `ThemeProvider` component. Sibling to `components/`, not nested in it, since it's app
  wiring more than it is a component.
- `src/themes/` — `index.ts` (`themeNames`) plus one flat `<name>.scss` per theme, no
  subfolders. Also a `components/` sibling — nothing in it is a React component, just
  names and styles. Per-component/page styles live in `src/styles/`, next to `variants.scss`.

## Two layers of components

**Primitives** (`primitives/`) — `Div`, `Label`, `Button`, `Input`, `TextField`,
`Select`, `RadioButton`, `Checkbox`, `ToggleSwitch`. One fixed implementation,
plain HTML elements with no logic of their own — they just forward props to DOM
attributes/events (`onClicked` → `onClick`, `onChanged` → `onChange` + extracting
`e.target.value`, etc.) and accept `style`/`className`/`variant` on top via
`ThemedProps<T>`. Every theme uses this same implementation — themes differ only by CSS
(see below), never by React component.

**Composed components** (`ChatEntry`, `ChatMessage`, `ToolMessage`, `UserInput`, ...) —
built out of primitives, imported directly (`import { Div, Label } from './primitives'`)
same as anything else would. They automatically look different per theme because the
primitives they're built from do, via `variant`/CSS — no theme-awareness needed in the
composed component itself.

Props are flattened: `ThemedProps<T> = T & OverrideThemeParams`, so a primitive's own
props (`text`, `onClicked`, ...) and the shared overrides (`style`, `className`,
`variant`) all sit in one object, and JSX children work normally (`<Div>...</Div>`, not
some nested `props.children` wrapper).

## Theme names

`src/themes/index.ts` exports `themeNames` — the flat list of valid `data-theme` values
(see the file for the current list). `context/theme-provider.tsx` exposes
`themeName`/`setThemeName`/`themeNames` via `useTheme()`; setting `themeName` updates
`:root[data-theme="..."]`, which is what everything below keys off. See the Quickstart
above for what adding one actually involves.

## The `variant` / `data-theme` system

`OverrideThemeParams.variant?: 'primary' | 'secondary' | 'tertiary'` exists so composed
components can express "this should look highlighted/emphasized" without hardcoding a
color themselves (see `ChatEntry`'s selected state). A primitive does nothing with
`variant` except forward it as `data-variant="..."` on its root DOM node — zero logic,
same as forwarding `onClick`.

Two files own the rest of the mechanism, and **they are not interchangeable**:

- **`styles/variants.scss`** (one file, shared by every theme, written once) — maps
  `[data-variant="primary"]` etc. to CSS custom properties: `[data-variant="primary"] {
  background: var(--color-primary); }`. This file never changes when a theme is added.
- **`themes/<name>.scss`** (one per theme, flat — no subfolder) — defines what those
  variables *equal* for that theme, scoped under `:root[data-theme="<name>"]`. This is
  the only thing a theme needs to write to participate in the variant system.

### Why the split — read this before you "simplify" it

Vite bundles **all** imported theme CSS together, regardless of which theme is active at
runtime — the bundler has no way to know that at build time, only React knows it, and
only after the page has loaded. So every rule from every theme's CSS coexists in the
same final stylesheet at all times. That fact is what makes the patterns below safe or
not.

#### Safe: a theme's own colors, scoped under its own `data-theme`
```scss
[data-theme="dark"] {
  --color-primary: #F0EFEA;
  --color-secondary: #1D1E18;
  --color-tertiary: #AAD2BA;
}
```
Safe by construction: every rule that reads `var(--color-primary)` etc. resolves it
through normal CSS inheritance from whatever element the attribute is actually on, so
even though every theme's `[data-theme="..."]` block ships in the same bundle, only the
element(s) actually carrying that value pick it up. `ThemeProvider` sets this on
`document.documentElement` (which also matches `:root`) for the app-wide theme, but
nothing about the selector requires that — a card that wants to preview one theme's
colors without switching the whole page can set `data-theme="..."` on its own wrapper
instead (see `ThemePreview`'s theme-picker cards). No collision is possible either way.

The same goes for anything else scoped under a theme's own `data-theme` that isn't a
variant color — fonts, spacing, border-radius, whatever look/feel that theme wants:
```scss
[data-theme="dark"] input { font-family: monospace; }
```
It's scoped, so it can't leak into another theme, and since it's not a color, it's not
double-managing something `styles/variants.scss` already covers.

#### Unsafe: a bare `[data-variant="..."]` rule in a theme file
```scss
/* themes/dark.scss — wrong */
[data-variant="primary"] { background: blue; }
```
This selector has no `data-theme` scoping, so it matches **every** theme's elements, all
the time, regardless of which theme is actually active. Two themes doing this collide for
real — cascade/source-order picks a winner, not "whichever theme the user selected." This
belongs in `styles/variants.scss`, and only there, exactly once, forever.

#### Redundant: redeclaring a variant color under a theme's own scope
```scss
/* themes/dark.scss — pointless */
[data-theme="dark"] .div { background-color: var(--color-primary); }
```
Not dangerous — it's scoped, so no collision — just redundant, and it desyncs from
`styles/variants.scss` the moment someone changes the shared mapping without also updating this
copy. If it's a variant color, that's `styles/variants.scss`'s job, not a theme file's.

### Code blocks

Syntax-highlighted code (chat messages and the code file preview) takes its colors from the
same three variables: `src/utils/code-theme.ts` is a Prism token theme made of
`var(--color-primary)` / `var(--color-tertiary)` and `color-mix()` blends of the two, handed
to `react-syntax-highlighter`. A new theme needs nothing for code; keep its accent distinct from
its text color, since that is the only thing telling a keyword from plain text.

### The one-sentence version

**A theme only ever *fills in values* (`--color-primary: ...`); `styles/variants.scss` is the
only file allowed to *wire* those values to real CSS properties via `[data-variant]`.**

### Backgrounds

What sits behind the pages is chosen in Settings → Background and kept per browser, like the theme
(`ThemeContext.background`: an id, a brightness and a speed). `off` is the plain page color, `dots`
is the static dot grid (`_mixins.scss`'s `dot-grid`, switched on by `data-dots` on `<html>`), and
every other id is an animated background drawn on one canvas behind everything
(`components/ascii-background.tsx`). Animated backgrounds take their colors from the same three
variables, read from `<html>` whenever `data-theme` changes, so a theme needs nothing for them.

They live in `src/backgrounds/`:
- `engine/scene.ts` — `Scene`, the one renderer: a grid of characters over the window, each cell a
  character and a level (1–4 the accent at rising opacity, 5 the text color), drawn as text one
  level at a time; plus what an effect needs to know about the page (the floor above the composer,
  the area right of the sidebar, a centered panel's sides).
- `engine/runner.ts` — loads the chosen effect, draws it at its own frame rate, stops while the
  tab is hidden, a quarter speed with "reduce motion".
- `engine/random.ts` — `rand` for what may change, `mulberry` / `hash` / `noise` for what must
  come out the same every frame (a tree's leaves, a ridge).
- `layers/` — pieces several effects share (stars, fireflies, embers, clouds, a static layer).
- `effects/` — one file per family; each exports a factory taking its options and returning
  `(scene) => ({init, draw})`, with all its state private to it.
- `index.ts` — the registry the picker lists: id, name, group, frame rate, a description and a
  `load` that imports the effect's file only when it is chosen.

A new background is a file in `effects/` (or a new set of options for an existing one) and one
entry in `index.ts`.

The scenes are drawn as light on dark: stars, the moon, fire and lit windows are bright marks on a
dark sky. On a light theme the same marks come out dark on a light sky, like a photo negative.
That is a decision, not a bug: a light theme keeps its light page, and the scenes are not redrawn
for it. What *is* corrected is how strongly they show: `inkFor` in `engine/scene.ts` raises the
ink where the accent is close to the background or the page is light, so a light theme shows them
about as clearly as the dark themes they were tuned on.
