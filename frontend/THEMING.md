# Theming

## Quickstart: adding a theme
1. Create `src/themes/<name>.css`:
   ```css
   :root[data-theme="<name>"] {
     --color-primary: #...;
     --color-secondary: #...;
     --color-tertiary: #...;
   }
   ```
2. In `src/themes/index.ts`, import that file and add `<name>` to `themeNames`:
   ```ts
   import './<name>.css'
   // ...
   export const themeNames = [/* existing names */, '<name>'] as const
   ```

That's it — nothing in `primitives/` or any composed component ever needs to change; every
UI piece already re-colors itself off those three variables. Read on for why it's built
this way and the rules that keep multiple themes from colliding.

---

How theming is laid out across `src/`, and — the part that isn't obvious from reading any
single file — the rules around `variant`/`data-theme` CSS that keep multiple themes from
silently colliding with each other. Read this before adding a new theme or touching
`variants.css`.

- `components/primitives/` — the fixed component set, see below. Stays under
  `components/` since it's the one piece here that's actually made of React components.
- `src/context/` — `ThemeContext.ts`/`ThemeProvider.tsx`/`useTheme.ts`, split into three
  files (not because any of them is complex) because oxlint's `only-export-components`
  fast-refresh rule flags any file that exports a component alongside a non-component
  value — the context object and the hook both count, so they can't share a file with the
  `ThemeProvider` component. Sibling to `components/`, not nested in it, since it's app
  wiring more than it is a component.
- `src/themes/` — `index.ts` (`themeNames`) plus one flat `<name>.css` per theme, no
  subfolders. Also a `components/` sibling — nothing in it is a React component, just
  names and CSS.

## Two layers of components

**Primitives** (`primitives/`) — `Div`, `Label`, `Button`, `Input`, `TextField`,
`Select`, `RadioButton`, `Checkbox`, `ToggleSwitch`, `Icon`. One fixed implementation,
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
(see the file for the current list). `context/ThemeProvider.tsx` exposes
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

- **`variants.css`** (one file, shared by every theme, written once) — maps
  `[data-variant="primary"]` etc. to CSS custom properties: `[data-variant="primary"] {
  background: var(--color-primary); }`. This file never changes when a theme is added.
- **`themes/<name>.css`** (one per theme, flat — no subfolder) — defines what those
  variables *equal* for that theme, scoped under `:root[data-theme="<name>"]`. This is
  the only thing a theme needs to write to participate in the variant system.

### Why the split — read this before you "simplify" it

Vite bundles **all** imported theme CSS together, regardless of which theme is active at
runtime — the bundler has no way to know that at build time, only React knows it, and
only after the page has loaded. So every rule from every theme's CSS coexists in the
same final stylesheet at all times. That fact is what makes the patterns below safe or
not.

#### Safe: a theme's own colors, scoped under its own `data-theme`
```css
:root[data-theme="dark"] {
  --color-primary: #F0EFEA;
  --color-secondary: #1D1E18;
  --color-tertiary: #AAD2BA;
}
```
Safe by construction: `data-theme` can only equal one string on the real DOM at once, so
even though every theme's `:root[data-theme="..."]` block ships in the same bundle, only
one of them ever actually matches at a time. No collision is possible.

The same goes for anything else scoped under a theme's own `data-theme` that isn't a
variant color — fonts, spacing, border-radius, whatever look/feel that theme wants:
```css
[data-theme="dark"] input { font-family: monospace; }
```
It's scoped, so it can't leak into another theme, and since it's not a color, it's not
double-managing something `variants.css` already covers.

#### Unsafe: a bare `[data-variant="..."]` rule in a theme file
```css
/* themes/dark.css — wrong */
[data-variant="primary"] { background: blue; }
```
This selector has no `data-theme` scoping, so it matches **every** theme's elements, all
the time, regardless of which theme is actually active. Two themes doing this collide for
real — cascade/source-order picks a winner, not "whichever theme the user selected." This
belongs in `variants.css`, and only there, exactly once, forever.

#### Redundant: redeclaring a variant color under a theme's own scope
```css
/* themes/dark.css — pointless */
[data-theme="dark"] .div { background-color: var(--color-primary); }
```
Not dangerous — it's scoped, so no collision — just redundant, and it desyncs from
`variants.css` the moment someone changes the shared mapping without also updating this
copy. If it's a variant color, that's `variants.css`'s job, not a theme file's.

### The one-sentence version

**A theme only ever *fills in values* (`--color-primary: ...`); `variants.css` is the
only file allowed to *wire* those values to real CSS properties via `[data-variant]`.**
