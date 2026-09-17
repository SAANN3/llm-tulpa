import { ChevronDown, ChevronRight } from 'pixelarticons/react'

import '../styles/ToolMessage.scss'
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
    <Div className="vbox tool-message__block">
      <Label variant="secondary" className="tool-message__block-label" text={label} />
      <pre className="mono tool-message__pre">{text}</pre>
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
    <Div className="tool-message">
      <Div className="vbox tool-message__card">
        <Div onClick={onToggle} className="list-row tool-message__header">
          <span className="tool-message__dot" />
          <Label className="mono tool-message__name" text={tool_name} />
          {!expanded ? (
            <Label variant="secondary" className="mono tool-message__summary" text={argsSummary} />
          ) : (
            <Div className="tool-message__spacer" />
          )}
          {!expanded && resultChip ? (
            <Label variant="secondary" className="mono tool-message__chip" text={resultChip} />
          ) : null}
          <span className="tool-message__caret">
            {expanded ? <ChevronDown width={14} height={14} /> : <ChevronRight width={14} height={14} />}
          </span>
        </Div>
        {expanded ? (
          <Div className="vbox tool-message__detail">
            {argsText ? <DetailBlock label="ARGUMENTS" text={argsText} /> : null}
            <DetailBlock label="RESULT" text={contentText} />
            {success === false && err ? <Label variant="secondary" className="tool-message__err" text={err} /> : null}
          </Div>
        ) : null}
      </Div>
      <Label variant="secondary" className="tool-message__time" text={new Date(created_at).toLocaleTimeString()} />
    </Div>
  )
}
