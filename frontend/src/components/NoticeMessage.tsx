import { Div, Label } from './primitives'

export interface NoticeMessageProps {
  content: string
}

/** A `notice` — the backend telling the chat something happened (a background job
 * finished) rather than anyone in it saying something. Centered and muted, like a
 * `DateSeparator`, so it reads as a marker in the timeline rather than as a turn. */
export function NoticeMessage({ content }: NoticeMessageProps) {
  return (
    <Div style={{ display: 'flex', justifyContent: 'center', padding: '2px 0' }}>
      <Div
        style={{
          maxWidth: '70ch',
          padding: '6px 14px',
          borderRadius: 8,
          background: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
        }}
      >
        <Label text={content} style={{ fontSize: 13, opacity: 0.75, lineHeight: 1.45 }} />
      </Div>
    </Div>
  )
}
