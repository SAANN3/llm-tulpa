import axios from 'axios'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { Div, Label } from '../components/primitives'
import { Sidebar } from '../components/Sidebar'
import { ThinkingAnimation } from '../components/ThinkingAnimation'
import { UserInput } from '../components/UserInput'
import { useChats } from '../hooks/useChats'
import { useDocumentTitle } from '../hooks/useDocumentTitle'
import { usePrompts } from '../hooks/usePrompts'
import { setPendingPrompt } from '../pendingPrompt'

function Home() {
  useDocumentTitle('Llm-tulpa')
  const navigate = useNavigate()
  const { greet, inputExample, chatName } = usePrompts()
  const { createChat } = useChats()
  const [greeting, setGreeting] = useState('')
  const [greetingLoading, setGreetingLoading] = useState(true)
  const [placeholder, setPlaceholder] = useState('')
  const [creating, setCreating] = useState(false)

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

  const onSend = async (prompt: string, think: boolean) => {
    setCreating(true)
    try {
      const name = await chatName(prompt)
      const chat = await createChat(name)
      setPendingPrompt(chat.id, prompt, think)
      navigate(`/chat?id=${chat.id}`)
    } finally {
      setCreating(false)
    }
  }

  return (
    <Div className="page">
      <Sidebar />
      <Div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', padding: 8, gap: 8 }}>
        <Div className="center vbox" style={{ flex: 1, minHeight: 0, gap: 8 }}>
          <ThinkingAnimation isPlaying={greetingLoading || creating} />
          <Label text={greeting} style={{ textAlign: 'center' }} />
          <UserInput
            blocked={creating}
            onSended={onSend}
            placeholder={placeholder || undefined}
            clearOnSend={false}
            style={{ width: 480, maxWidth: '100%', marginTop: 24 }}
          />
        </Div>
      </Div>
    </Div>
  )
}

export default Home
