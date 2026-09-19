import {useRef} from 'react'
import {useLocation, useNavigate, useSearchParams} from 'react-router-dom'
import '../styles/sidebar.scss'
import {ChatEntry} from './chat-entry.tsx'
import type {LazyListHandle} from './lazy-list.tsx'
import {LazyList} from './lazy-list.tsx'
import {Button, Div, Label} from './primitives'
import type {ChatOut} from '../api/chats/types'
import {useAuth} from '../context/use-auth.ts'
import {useChats} from '../hooks/use-chats.ts'
import {daysBefore} from '../utils/dates'

const RECENCY_GROUPS = ['Today', 'Yesterday', 'Earlier'] as const
type RecencyGroup = (typeof RECENCY_GROUPS)[number]

/** Which of the three recency headings a chat's last-updated time falls under */
const recencyGroup = (updatedAt: string): RecencyGroup => {
    const diffDays = daysBefore(new Date(updatedAt))

    if (diffDays <= 0) return 'Today'
    if (diffDays === 1) return 'Yesterday'
    return 'Earlier'
};

/** Buckets chats into the three recency groups, dropping empty ones */
const groupByRecency = (chats: ChatOut[]): [RecencyGroup, ChatOut[]][] => {
    const buckets = new Map<RecencyGroup, ChatOut[]>()
    for (const chat of chats) {
        const group = recencyGroup(chat.updated_at)
        if (!buckets.has(group)) buckets.set(group, [])
        buckets.get(group)!.push(chat)
    }
    return RECENCY_GROUPS.filter((g) => buckets.has(g)).map((g) => [g, buckets.get(g)!])
};

/** The chat-list sidebar, shared between the chat and home routes */
export const Sidebar = () => {
    const navigate = useNavigate()
    const location = useLocation()
    const {user, logout} = useAuth()
    const [searchParams] = useSearchParams()
    const idParam = searchParams.get('id')
    const selectedChatId = idParam == null ? null : Number(idParam)

    const chatsListRef = useRef<LazyListHandle>(null)
    const {chats, loadOlder, rename, delete: deleteChat} = useChats(() => chatsListRef.current?.jumpToTop())

    const selectChat = (id: number) => navigate(`/chat?id=${id}`)

    const onNewChat = () => navigate('/')

    return (
        <Div className="vbox sidebar">
            <Button variant="primary" text="New chat" onClicked={onNewChat}/>
            <Label className="section-heading" text="Utils"/>
            <Div className="vbox">
                <ChatEntry label="Settings" selected={false} onClicked={() => navigate('/settings')}/>
                <ChatEntry label="Plugins" selected={false} onClicked={() => navigate('/plugins')}/>
                {user?.role === 'owner' ? (
                    <ChatEntry label="Users" selected={false} onClicked={() => navigate('/users')}/>
                ) : null}
                <ChatEntry label="Log out" selected={false} onClicked={logout}/>
            </Div>
            <Label className="section-heading" text="Chats"/>
            <LazyList ref={chatsListRef} onBottomReached={loadOlder} className="sidebar__list">
                <Div className="vbox sidebar__groups">
                    {groupByRecency(chats).map(([group, groupChats]) => (
                        <Div className="vbox sidebar__group" key={group}>
                            <Label className="section-heading sidebar__group-heading" text={group}/>
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
};
