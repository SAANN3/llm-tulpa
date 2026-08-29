import { useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'

import { Attachment } from './Attachment'
import { Button, Div, Label } from './primitives'

export interface ChatMessageProps {
  role: 'user' | 'assistant'
  content: string
  created_at: string
  thinking?: string | null
  thought_duration_ms?: number | null
  /** Base64-encoded image data (no data-URL prefix), if any — only ever set on a `user` message. */
  images?: string[]
}

function formatThoughtDuration(ms: number): string {
  const totalSeconds = ms / 1000
  if (totalSeconds < 60) return `Thought for ${totalSeconds.toFixed(1)}s`

  const minutes = Math.floor(totalSeconds / 60)
  const seconds = Math.round(totalSeconds % 60)
  return `Thought for ${minutes}m ${seconds}s`
}

/** One chat message bubble — aligned by `role`. Field names match `MessageOut`/`ChatOut` from the api layer so a fetched message can be spread straight in. */
export function ChatMessage({ role, content, created_at, thinking, thought_duration_ms, images }: ChatMessageProps) {
  const isUser = role === 'user'
  const [showThinking, setShowThinking] = useState(false)

  return (
    <Div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: isUser ? 'flex-end' : 'flex-start',
      }}
    >
      <Div
        className={isUser ? undefined : 'vbox'}
        style={
          isUser
            ? {
                maxWidth: '68%',
                background: 'var(--color-surface-strong)',
                border: '1px solid var(--color-border)',
                borderRadius: '12px 12px 4px 12px',
                padding: '10px 14px',
                lineHeight: 1.5,
              }
            : {
                maxWidth: '70ch',
                borderLeft: '2px solid var(--color-border)',
                paddingLeft: 14,
                fontSize: 15,
                lineHeight: 1.55,
                gap: 8,
              }
        }
      >
        {images && images.length > 0 ? (
          <Div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: content ? 8 : 0 }}>
            {images.map((image, index) => (
              <Attachment key={index} image={image} size={240} />
            ))}
          </Div>
        ) : null}
        <div className="markdown">
          <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]}>{content}</ReactMarkdown>
        </div>
        {thinking ? (
          <Div className="vbox" style={{ gap: 4, maxWidth: '100%' }}>
            <Button
              variant="secondary"
              text={`${showThinking ? '▾' : '▸'} ${thought_duration_ms != null ? formatThoughtDuration(thought_duration_ms) : 'Thinking'}`}
              onClicked={() => setShowThinking((v) => !v)}
              style={{ fontSize: 12, padding: '3px 9px', borderRadius: 5, marginTop: 8, alignSelf: 'flex-start', border: '1px solid var(--color-border)' }}
            />
            {showThinking ? (
              <Div
                className="panel"
                style={{
                  padding: 12,
                  fontSize: 13,
                  opacity: 0.85,
                  whiteSpace: 'pre-wrap',
                  background: 'var(--color-surface)',
                  border: '1px solid var(--color-border)',
                }}
              >
                {thinking}
              </Div>
            ) : null}
          </Div>
        ) : thought_duration_ms != null ? (
          <Label
            variant="secondary"
            text={formatThoughtDuration(thought_duration_ms)}
            style={{ fontSize: 12, opacity: 0.6, marginTop: 8 }}
          />
        ) : null}
        <Label
          variant="secondary"
          text={new Date(created_at).toLocaleTimeString()}
          style={{ fontSize: 11, opacity: 0.6, marginTop: 6, textAlign: isUser ? 'right' : undefined }}
        />
      </Div>
    </Div>
  )
}
