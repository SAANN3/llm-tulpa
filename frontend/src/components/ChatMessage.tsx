import { useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'

import { Button, Div, Label } from './primitives'

export interface ChatMessageProps {
  role: 'user' | 'assistant'
  content: string
  created_at: string
  thinking?: string | null
  thought_duration_ms?: number | null
}

function formatThoughtDuration(ms: number): string {
  const totalSeconds = ms / 1000
  if (totalSeconds < 60) return `Thought for ${totalSeconds.toFixed(1)}s`

  const minutes = Math.floor(totalSeconds / 60)
  const seconds = Math.round(totalSeconds % 60)
  return `Thought for ${minutes}m ${seconds}s`
}

/** One chat message bubble — aligned by `role`. Field names match `MessageOut`/`ChatOut` from the api layer so a fetched message can be spread straight in. */
export function ChatMessage({ role, content, created_at, thinking, thought_duration_ms }: ChatMessageProps) {
  const isUser = role === 'user'
  const [showThinking, setShowThinking] = useState(false)

  return (
    <Div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: isUser ? 'flex-end' : 'flex-start',
        padding: 8,
      }}
    >
      <div className="markdown">
        <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]}>{content}</ReactMarkdown>
      </div>
      {thinking ? (
        <Div className="vbox" style={{ gap: 4, marginBottom: 4, maxWidth: '100%' }}>
          <Button
            variant="secondary"
            text={`${showThinking ? '▾' : '▸'} ${thought_duration_ms != null ? formatThoughtDuration(thought_duration_ms) : 'Thinking'}`}
            onClicked={() => setShowThinking((v) => !v)}
            style={{ fontSize: 12, padding: '4px 10px', margin: '10px 0px 0px 0px' }}
          />
          {showThinking ? (
            <Div variant="primary" className="panel" style={{ padding: 12, fontSize: 13, opacity: 0.85, whiteSpace: 'pre-wrap' }}>
              {thinking}
            </Div>
          ) : null}
        </Div>
      ) : thought_duration_ms != null ? (
        <Label text={formatThoughtDuration(thought_duration_ms)} style={{ fontSize: 12, opacity: 0.6, marginBottom: 4 }} />
      ) : null}
      <Div
        style={{ padding: '2px 8px', borderRadius: 10, marginTop: 4 }}
      >
        <Label text={new Date(created_at).toLocaleTimeString()} style={{ fontSize: 11 }} />
      </Div>
    </Div>
  )
}
