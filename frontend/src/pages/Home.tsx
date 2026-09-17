import axios from 'axios'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'

import '../styles/Home.scss'
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

  const loading = greetingLoading || creating

  return (
    <Div className="page">
      <Sidebar />
      <Div className="home">
        <Div className="center vbox home__stage">
          <Mark spinning={loading} />
          {loading ? (
            <Label
              className="status-line home__status"
              variant="secondary"
              text={creating ? 'Starting a chat' : 'Thinking'}
            />
          ) : null}
          {!greetingLoading && greeting ? (
            <Label className="greeting home__greeting" text={greeting} />
          ) : null}
          <UserInput
            blocked={creating}
            onSended={onSend}
            placeholder={placeholder || undefined}
            clearOnSend={false}
            className="home__composer"
          />
        </Div>
      </Div>
    </Div>
  )
}

export default Home
