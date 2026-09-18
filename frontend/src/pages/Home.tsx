import axios from 'axios'
import { useEffect, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'

import type { ThinkChoice } from '../api/agent/types'
import { Div, Label } from '../components/primitives'
import { Mark } from '../components/Mark'
import { Sidebar } from '../components/Sidebar'
import { UserInput } from '../components/UserInput'
import { useChats } from '../hooks/useChats'
import { useDocumentTitle } from '../hooks/useDocumentTitle'
import { usePrompts } from '../hooks/usePrompts'
import { setPendingPrompt } from '../utils/pendingPrompt'

function Home() {
  useDocumentTitle('Llm-tulpa')
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const { greet, inputExample, chatName } = usePrompts()
  const { createChat } = useChats()
  const [greeting, setGreeting] = useState('')
  const [greetingLoading, setGreetingLoading] = useState(true)
  const [placeholder, setPlaceholder] = useState('')
  const [creating, setCreating] = useState(false)

  // Seed the input from a ?prompt= query param (set by the launcher) and auto-send.
  const launchPrompt = searchParams.get('prompt')

  const onSend = async (prompt: string, think: ThinkChoice, images: string[], fileIds: number[]) => {
    setCreating(true)
    try {
      const name = await chatName(prompt, images)
      const chat = await createChat(name)
      setPendingPrompt(chat.id, prompt, think, images, fileIds)
      navigate(`/chat?id=${chat.id}`)
    } finally {
      setCreating(false)
    }
  }

  useEffect(() => {
    if (launchPrompt && !creating) {
      const decoded = decodeURIComponent(launchPrompt)
      void onSend(decoded, true, [], [])
    }
  }, [])

  // Both `greet` and `inputExample` can take many seconds on a cache miss. Without
  // aborting on unmount, navigating away mid-request (and back, repeatedly) leaves the
  // old requests running in the browser — each one holding a connection slot against the
  // backend's origin. Browsers cap those per-origin (6 for HTTP/1.1, which this app uses
  // — plain http://, no TLS/h2), so stacking up enough orphaned slow requests can leave
  // no slot free for a fresh page's own `/api/chats` fetch until an old one finally
  // finishes, which reads as "the chat list is empty" for however long that takes.
  useEffect(() => {
    const controller = new AbortController()

    greet(controller.signal)
      .then((text) => {
        setGreeting(text)
        setGreetingLoading(false)
      })
      .catch((err) => {
        if (!axios.isCancel(err)) throw err
      })

    return () => controller.abort()
  }, [greet])

  useEffect(() => {
    const controller = new AbortController()

    inputExample(controller.signal)
      .then(setPlaceholder)
      .catch((err) => {
        if (!axios.isCancel(err)) throw err
      })

    return () => controller.abort()
  }, [inputExample])

  const loading = greetingLoading || creating

  return (
    <Div className="page">
      <Sidebar />
      <Div
        style={{
          flex: 1,
          minHeight: 0,
          display: 'flex',
          flexDirection: 'column',
          padding: '8px 24px 64px',
        }}
      >
        <Div className="center vbox" style={{ flex: 1, minHeight: 0, gap: 26 }}>
          <Mark spinning={loading} />
          {loading ? (
            <Label
              className="status-line"
              variant="secondary"
              text={creating ? 'Starting a chat' : 'Thinking'}
              style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: '0.1em' }}
            />
          ) : null}
          {!greetingLoading && greeting ? (
            <Label
              className="greeting"
              text={greeting}
              style={{ fontSize: 26, lineHeight: 1.4, textAlign: 'center', maxWidth: '24ch', textWrap: 'pretty' }}
            />
          ) : null}
          <UserInput
            blocked={creating}
            onSended={onSend}
            placeholder={placeholder || undefined}
            clearOnSend={false}
            style={{ width: 520, maxWidth: '100%' }}
          />
        </Div>
      </Div>
    </Div>
  )
}

export default Home
