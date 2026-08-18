import { Div, Label } from './primitives'

export interface ToolMessageProps {
  tool_name: string
  content: unknown
  created_at: string
  arguments?: Record<string, unknown>
  /**
   * `success`/`err` come from a live `useTool` response only — persisted messages
   * (`MessageOut`, from `getMessages`) don't carry them, so historical tool messages
   * render with just `tool_name`/`content` and no succeeded/failed framing.
   */
  success?: boolean
  err?: string | null
}

/** How tall a single scrollable block (arguments or content) is allowed to get before
 * it scrolls internally instead of pushing the rest of the chat around — a big file
 * read or directory listing stays fully there, just not fully on screen at once. */
const MAX_BLOCK_HEIGHT = 180

/** Rough proxy for "long enough that it's probably scrolling" — there's no cheap way
 * to know the actual rendered height without a ref/observer for what's meant to be a
 * small affordance, so this estimates off text length instead; the `panel` border
 * alone doesn't tell you a box scrolls, especially wherever the OS/browser hides an
 * idle scrollbar. */
const LIKELY_SCROLLS_CHARS = 600

/** A bounded, independently-scrollable block of pre-formatted text — bordered (via the
 * shared `panel` class, so it stays theme-aware) so it visually reads as its own box,
 * with a small caption underneath once it's long enough to likely need scrolling. */
function ScrollBlock({ text }: { text: string }) {
  return (
    <Div className="vbox" style={{ gap: 2 }}>
      <Div className="panel" style={{ maxHeight: MAX_BLOCK_HEIGHT, overflow: 'auto', padding: 8 }}>
        <Label text={text} style={{ opacity: 0.8, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }} />
      </Div>
      {text.length > LIKELY_SCROLLS_CHARS ? (
        <Label text="scrollable — showing partial content" style={{ fontSize: 11, opacity: 0.5 }} />
      ) : null}
    </Div>
  )
}

/** Reports one tool call's result — field names match `UseToolOut` from the api layer so a tool result can be spread straight in. */
export function ToolMessage({ tool_name, content, created_at, arguments: args, success, err }: ToolMessageProps) {
  const contentText = typeof content === 'string' ? content : JSON.stringify(content, null, 2)
  const argsText = args && Object.keys(args).length > 0 ? JSON.stringify(args, null, 2) : null

  return (
    <Div style={{ display: 'flex', flexDirection: 'column', gap: 4, padding: 8, opacity: 0.7 }}>
      <Label text={success == null ? tool_name : success ? `${tool_name} succeeded` : `${tool_name} failed`} />
      {argsText ? <ScrollBlock text={argsText} /> : null}
      <ScrollBlock text={contentText} />
      {success === false && err ? <Label text={err} style={{ opacity: 0.7 }} /> : null}
      <Label text={new Date(created_at).toLocaleTimeString()} style={{ fontSize: 12, opacity: 0.6 }} />
    </Div>
  )
}
