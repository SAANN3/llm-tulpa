import { useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'
import { ChevronDown, ChevronRight } from 'pixelarticons/react'

import '../styles/ChatMessage.scss'
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
  /** Ids of already-uploaded (non-image) files attached, if any — only ever set on a `user` message. */
  file_ids?: number[]
}

function formatThoughtDuration(ms: number): string {
  const totalSeconds = ms / 1000
  if (totalSeconds < 60) return `Thought for ${totalSeconds.toFixed(1)}s`

  const minutes = Math.floor(totalSeconds / 60)
  const seconds = Math.round(totalSeconds % 60)
  return `Thought for ${minutes}m ${seconds}s`
}

/** One chat message bubble — aligned by `role`. Field names match `MessageOut`/`ChatOut` from the api layer so a fetched message can be spread straight in. */
export function ChatMessage({ role, content, created_at, thinking, thought_duration_ms, images, file_ids }: ChatMessageProps) {
  const isUser = role === 'user'
  const [showThinking, setShowThinking] = useState(false)

  return (
    <Div className={`chat-message ${isUser ? 'chat-message--user' : 'chat-message--assistant'}`}>
      <Div className={isUser ? 'chat-message__bubble' : 'vbox chat-message__body'}>
        {(images && images.length > 0) || (file_ids && file_ids.length > 0) ? (
          <Div className={`chat-message__attachments${content ? ' chat-message__attachments--spaced' : ''}`}>
            {images?.map((image, index) => <Attachment key={`image-${index}`} kind="image" image={image} size={140} />)}
            {file_ids?.map((id) => <Attachment key={`file-${id}`} kind="file" fileId={id} size={140} />)}
          </Div>
        ) : null}
        <div className="markdown">
          <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]}>{content}</ReactMarkdown>
        </div>
        {thinking ? (
          <Div className="vbox chat-message__thinking">
            <Button
              className="chat-message__thinking-toggle"
              variant="secondary"
              onClicked={() => setShowThinking((v) => !v)}
            >
              {showThinking ? <ChevronDown width={13} height={13} /> : <ChevronRight width={13} height={13} />}
              <span>{thought_duration_ms != null ? formatThoughtDuration(thought_duration_ms) : 'Thinking'}</span>
            </Button>
            {showThinking ? <Div className="chat-message__thinking-body">{thinking}</Div> : null}
          </Div>
        ) : thought_duration_ms != null ? (
          <Label variant="secondary" className="chat-message__thought" text={formatThoughtDuration(thought_duration_ms)} />
        ) : null}
        <Label
          variant="secondary"
          className={`chat-message__time${isUser ? ' chat-message__time--user' : ''}`}
          text={new Date(created_at).toLocaleTimeString()}
        />
      </Div>
    </Div>
  )
}
