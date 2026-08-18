import { useRef } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import { ChatEntry } from './ChatEntry'
import type { LazyListHandle } from './LazyList'
import { LazyList } from './LazyList'
import { Button, Div, Label } from './primitives'
import { useChats } from '../hooks/useChats'

/** The chat-list sidebar — shared between `/chat` (a chat open) and `/` (none open yet). Fully self-contained: fetches its own chat list and handles all of its own navigation, so pages just render it with no props. */
export function Sidebar() {
  const navigate = useNavigate()
  const location = useLocation()
  const [searchParams] = useSearchParams()
  const idParam = searchParams.get('id')
  const selectedChatId = idParam == null ? null : Number(idParam)

  const chatsListRef = useRef<LazyListHandle>(null)
  const { chats, loadOlder, rename, delete: deleteChat } = useChats(() => chatsListRef.current?.jumpToTop())

  const selectChat = (id: number) => navigate(`/chat?id=${id}`)

  const onNewChat = () => navigate('/')

  return (
    <Div className="bordered-right vbox" style={{ width: 240, minWidth: 240, flexShrink: 0, gap: 4, padding: 8 }}>
      <Button text="New chat" onClicked={onNewChat} />
      <Label text="Utils" />
      <Div className="vbox">
        <ChatEntry label="Settings" selected={false} onClicked={() => navigate('/settings')} />
      </Div>
      <Label text="Chats" />
      <LazyList ref={chatsListRef} onBottomReached={loadOlder} style={{ flex: 1, minHeight: 0 }}>
        {chats.map((c) => (
          <ChatEntry
            key={c.id}
            label={c.name}
            selected={c.id === selectedChatId}
            onClicked={() => selectChat(c.id)}
            onRename={(name) => rename(c.id, name)}
            onDelete={() => {
              deleteChat(c.id).then(() => {
                if (location.pathname === '/chat' && selectedChatId === c.id) navigate('/')
              })
            }}
          />
        ))}
      </LazyList>
    </Div>
  )
}
