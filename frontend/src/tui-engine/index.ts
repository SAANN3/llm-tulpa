// The character-grid canvas that both the animated backgrounds and the interface widgets draw through.
//
// Using it: widgets go inside a <TuiCanvas>; one canvas per region of the page, not per widget.
//   <TuiCanvas><TuiBox title="Disk" w="100%" h={6}><TuiText text="42 GB free" x={1} y={0}/></TuiBox></TuiCanvas>
// Sizes and places are cells (12), a share of the parent ('60%', '100% - 2'), px ('120px') or, for a place, 'center'.
// They resolve against the parent's inner area before every frame, so a widget draws from its own w/h and needs no
// resize code. A component of your own is <TuiCustom onRender={(scene, time, widget) => ...}/>, drawing with
// scene.put / scene.write from widget.getGlobalPos(). The canvas draws on demand by default; anything that moves on its
// own needs renderMode={TuiRenderMode.Continuous}. Levels: 0 empty, 1-4 the accent at rising opacity, 5 the text color;
// never a literal color. Keys reach the canvas only while it has focus (Tab, Enter/Space, Escape).
//
// Writing a widget: copy primitives/scanner-bar.ts and react/tui-scanner-bar.tsx. The options are one interface shared
// by the widget and its component, the defaults live only in the widget's configure(), every non-obvious number is a
// named constant, an option is either read by the drawing or not there, an option has one scale, text goes through
// scene.write (an emoji or CJK character is two cells). Check it with a throwaway harness page and a screenshot.
// Core
export * from './core/types.ts'
export * from './core/color.ts'
export * from './core/scene.ts'
export * from './core/widget.ts'
export * from './core/layout.ts'
export * from './core/text-cells.ts'
export * from './core/pixel-ratio.ts'

// Widgets
export * from './primitives/box.ts'
export * from './primitives/text.ts'
export * from './primitives/button.ts'
export * from './primitives/scanner-bar.ts'

// React
export * from './react/tui-canvas.tsx'
export * from './react/tui-box.tsx'
export * from './react/tui-text.tsx'
export * from './react/tui-button.tsx'
export * from './react/tui-custom.tsx'
export * from './react/tui-scanner-bar.tsx'
export * from './react/use-tui-scene.ts'
export * from './react/use-tui-widget.ts'
