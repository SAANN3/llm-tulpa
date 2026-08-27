import { useRef } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import { ChatEntry } from './ChatEntry'
import type { LazyListHandle } from './LazyList'
import { LazyList } from './LazyList'
import { Button, Div, Label } from './primitives'
import type { ChatOut } from '../api/chats/types'
import { useChats } from '../hooks/useChats'
import { daysBefore } from '../utils/dates'

const RECENCY_GROUPS = ['Today', 'Yesterday', 'Earlier'] as const
type RecencyGroup = (typeof RECENCY_GROUPS)[number]

/** Which of the three recency headings a chat's `updated_at` falls under — calendar-day
 * based (not "last 24h"), so a chat from yesterday morning reads as "Yesterday" even if
 * it's less than 24h ago. */
function recencyGroup(updatedAt: string): RecencyGroup {
  const diffDays = daysBefore(new Date(updatedAt))

  if (diffDays <= 0) return 'Today'
  if (diffDays === 1) return 'Yesterday'
  return 'Earlier'
}

/** Buckets `chats` (already newest-active-first) into the three recency groups, dropping empty ones — grouping never reorders within a bucket. */
function groupByRecency(chats: ChatOut[]): [RecencyGroup, ChatOut[]][] {
  const buckets = new Map<RecencyGroup, ChatOut[]>()
  for (const chat of chats) {
    const group = recencyGroup(chat.updated_at)
    if (!buckets.has(group)) buckets.set(group, [])
    buckets.get(group)!.push(chat)
  }
  return RECENCY_GROUPS.filter((g) => buckets.has(g)).map((g) => [g, buckets.get(g)!])
}

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
    <Div
      className="vbox"
      style={{
        width: 240,
        minWidth: 240,
        flexShrink: 0,
        gap: 6,
        padding: 8,
        background: 'var(--color-surface)',
        borderRight: '1px solid var(--color-border)',
      }}
    >
      <Button variant="primary" text="New chat" onClicked={onNewChat} />
      <Label className="section-heading" text="Utils" />
      <Div className="vbox">
        <ChatEntry label="Settings" selected={false} onClicked={() => navigate('/settings')} />
        <ChatEntry label="Plugins" selected={false} onClicked={() => navigate('/plugins')} />
      </Div>
      <Label className="section-heading" text="Chats" />
      <LazyList ref={chatsListRef} onBottomReached={loadOlder} style={{ flex: 1, minHeight: 0 }}>
        <Div className="vbox" style={{ gap: 4 }}>
          {groupByRecency(chats).map(([group, groupChats]) => (
            <Div className="vbox" key={group} style={{ gap: 2 }}>
              <Label className="section-heading" text={group} style={{ fontSize: 11 }} />
              {groupChats.map((c) => (
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
            </Div>
          ))}
        </Div>
      </LazyList>
    </Div>
  )
}
