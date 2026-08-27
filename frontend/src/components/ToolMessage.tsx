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
  /** Collapsed (summary row only) vs expanded (full arguments/result) — lifted to the
   * chat page as `expandedTools` so it survives whatever re-renders this message, not
   * local state here. */
  expanded: boolean
  onToggle: () => void
}

const ID_KEYS = ['path', 'file', 'filename', 'dir', 'directory', 'url', 'query', 'command', 'name', 'id']
const trunc = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + '…' : s)

/** One-line stand-in for a tool call's arguments: the most identifying scalar in them.
 * Frontend-only derivation from the same JSON already sent — no backend change, and no
 * per-tool knowledge (works the same for every tool). */
function describeArgs(args: Record<string, unknown>): string {
  for (const k of ID_KEYS) {
    const v = args[k]
    if (typeof v === 'string' && v) return trunc(v, 60)
  }
  for (const [k, v] of Object.entries(args)) {
    if (typeof v === 'string' && v.length <= 80) return trunc(v, 60)
    if (typeof v === 'number' || typeof v === 'boolean') return `${k} ${v}`
  }
  const n = Object.keys(args).length
  return n ? `${n} field${n === 1 ? '' : 's'}` : ''
}

/** Short status chip for a tool result: an error, a true flag, a count, or nothing. */
function describeResult(content: unknown): string {
  if (content === null || typeof content !== 'object') return trunc(String(content), 24)
  const obj = content as Record<string, unknown>
  if (obj.error) return 'error'
  for (const [k, v] of Object.entries(obj)) if (v === true) return k
  for (const [k, v] of Object.entries(obj)) if (typeof v === 'number') return `${k} ${v}`
  return 'ok'
}

/** A bounded, independently-scrollable, labelled block of pre-formatted JSON. */
function DetailBlock({ label, text }: { label: string; text: string }) {
  return (
    <Div className="vbox" style={{ gap: 4 }}>
      <Label variant="secondary" text={label} style={{ fontSize: 11, letterSpacing: '0.06em' }} />
      <pre
        className="mono"
        style={{
          margin: 0,
          fontSize: 12,
          lineHeight: 1.5,
          opacity: 0.85,
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-word',
          maxHeight: 180,
          overflow: 'auto',
        }}
      >
        {text}
      </pre>
    </Div>
  )
}

/** Reports one tool call's result — field names match `UseToolOut` from the api layer so a tool result can be spread straight in. Collapsed by default to a one-line summary (tool name, a derived argument summary, a derived result chip); expands to the full arguments/result JSON. */
export function ToolMessage({ tool_name, content, created_at, arguments: args, success, err, expanded, onToggle }: ToolMessageProps) {
  const contentText = typeof content === 'string' ? content : JSON.stringify(content, null, 2)
  const argsText = args && Object.keys(args).length > 0 ? JSON.stringify(args, null, 2) : null
  const argsSummary = args ? describeArgs(args) : ''
  const resultChip = success === false ? 'error' : describeResult(content)

  return (
    <Div style={{ maxWidth: '70ch', borderLeft: '2px solid var(--color-border)', paddingLeft: 14 }}>
      <Div
        className="vbox"
        style={{
          borderRadius: 8,
          overflow: 'hidden',
          border: '1px solid var(--color-border)',
          background: 'var(--color-surface)',
        }}
      >
        <Div
          onClick={onToggle}
          className="list-row"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            padding: '9px 12px',
            borderRadius: 0,
          }}
        >
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: '50%',
              backgroundColor: 'var(--color-tertiary)',
              flexShrink: 0,
            }}
          />
          <Label className="mono" text={tool_name} style={{ fontSize: 13 }} />
          {!expanded ? (
            <Label
              variant="secondary"
              className="mono"
              text={argsSummary}
              style={{ fontSize: 12, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
            />
          ) : (
            <Div style={{ flex: 1 }} />
          )}
          {!expanded && resultChip ? (
            <Label
              variant="secondary"
              className="mono"
              text={resultChip}
              style={{ fontSize: 11, border: '1px solid var(--color-border)', borderRadius: 4, padding: '1px 6px' }}
            />
          ) : null}
          <Label variant="secondary" text={expanded ? '▾' : '▸'} style={{ fontSize: 11 }} />
        </Div>
        {expanded ? (
          <Div className="vbox" style={{ gap: 10, padding: '10px 12px', borderTop: '1px solid var(--color-border)' }}>
            {argsText ? <DetailBlock label="ARGUMENTS" text={argsText} /> : null}
            <DetailBlock label="RESULT" text={contentText} />
            {success === false && err ? <Label variant="secondary" text={err} style={{ fontSize: 12 }} /> : null}
          </Div>
        ) : null}
      </Div>
      <Label
        variant="secondary"
        text={new Date(created_at).toLocaleTimeString()}
        style={{ fontSize: 11, opacity: 0.6, marginTop: 6 }}
      />
    </Div>
  )
}
