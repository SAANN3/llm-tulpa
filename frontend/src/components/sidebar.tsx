import {useEffect, useRef, useState} from 'react'
import {ArrowBarLeft, ArrowBarRight, Settings2, User, Folder, Plug, Logout, Analytics} from 'pixelarticons/react'
import {useLocation, useNavigate, useSearchParams} from 'react-router-dom'
import '../styles/sidebar.scss'
import {ChatEntry} from './chat-entry.tsx'
import {FolderPicker} from './folder-picker.tsx'
import type {LazyListHandle} from './lazy-list.tsx'
import {LazyList} from './lazy-list.tsx'
import {Popup} from './popup.tsx'
import {Button, Div, Label} from './primitives'
import {setChatFolder} from '../api/chats/set-folder'
import type {ChatOut} from '../api/chats/types'
import {useAuth} from '../context/use-auth.ts'
import {onSidebarWindowResized, setSidebarCollapsed, useSidebarCollapsed} from '../context/sidebar-state.ts'
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
    const [assigningChat, setAssigningChat] = useState<ChatOut | null>(null)
    const collapsed = useSidebarCollapsed()

    // The only resize caller: a crossing of the threshold collapses/expands, a resize that
    // stays on one side leaves a manual choice alone (see onSidebarWindowResized)
    useEffect(() => {
        const onResize = () => onSidebarWindowResized(window.innerWidth)
        window.addEventListener('resize', onResize)
        return () => window.removeEventListener('resize', onResize)
    }, [])

    const selectChat = (id: number) => navigate(`/chat?id=${id}`)

    const onNewChat = () => navigate('/')

    const toggle = (
        <Button
            className="sidebar__toggle"
            title={collapsed ? 'Expand sidebar' : 'Shrink sidebar'}
            onClicked={() => setSidebarCollapsed(!collapsed)}
        >
            {collapsed ? <ArrowBarRight width={18} height={18}/> : <ArrowBarLeft width={18} height={18}/>}
        </Button>
    )

    // Collapse-safe utils: hide plugins/users if not owner
    const utilsCollapsed = [
        {label: 'Settings', path: '/settings', icon: <Settings2 width={16} height={16}/>, action: () => navigate('/settings')},
        {label: 'Folders', path: '/folders', icon: <Folder width={16} height={16}/>, action: () => navigate('/folders')},
        {label: 'Usage', path: '/stats', icon: <Analytics width={16} height={16}/>, action: () => navigate('/stats')},
        ...(user?.role === 'owner' ? [
            {label: 'Plugins', path: '/plugins', icon: <Plug width={16} height={16}/>, action: () => navigate('/plugins')},
            {label: 'Users', path: '/users', icon: <User width={16} height={16}/>, action: () => navigate('/users')},
        ] : []),
        {label: 'Log out', path: '#logout', icon: <Logout width={16} height={16}/>, action: logout},
    ]

    // The shrunk rail: toggle + utils icons, no text
    if (collapsed) {
        return (
            <Div className="vbox sidebar sidebar--collapsed">
                <Div className="sidebar__top">
                    {toggle}
                </Div>
                <Div className="sidebar__utils-collapsed">
                    {utilsCollapsed.map(item => (
                        <Button
                            key={item.path}
                            className="sidebar__utils-item sidebar__utils-item--compact"
                            title={item.label}
                            onClicked={item.action}
                        >
                            {item.icon}
                        </Button>
                    ))}
                </Div>
            </Div>
        )
    }

    return (
        <Div className="vbox sidebar">
            <Div className="sidebar__top">
                {toggle}
                <Button variant="primary" className="sidebar__new-chat" text="New chat" onClicked={onNewChat}/>
            </Div>
            <Label className="section-heading" text="Utils"/>
            <Div className="vbox">
                <ChatEntry label="Settings" selected={location.pathname === '/settings'} icon={<Settings2 width={18} height={18}/>} onClicked={() => navigate('/settings')}/>
                <ChatEntry label="Folders" selected={location.pathname === '/folders'} icon={<Folder width={18} height={18}/>} onClicked={() => navigate('/folders')}/>
                <ChatEntry label="Usage" selected={location.pathname === '/stats'} icon={<Analytics width={18} height={18}/>} onClicked={() => navigate('/stats')}/>
                {user?.role === 'owner' ? (
                    <>
                        <ChatEntry label="Plugins" selected={location.pathname === '/plugins'} icon={<Plug width={18} height={18}/>} onClicked={() => navigate('/plugins')}/>
                        <ChatEntry label="Users" selected={location.pathname === '/users'} icon={<User width={18} height={18}/>} onClicked={() => navigate('/users')}/>
                    </>
                ) : null}
                <ChatEntry label="Log out" selected={false} icon={<Logout width={18} height={18}/>} onClicked={logout}/>
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
                                    onAssignFolder={() => setAssigningChat(c)}
                                />
                            ))}
                        </Div>
                    ))}
                </Div>
            </LazyList>
            <Popup open={assigningChat != null} onClose={() => setAssigningChat(null)} centered>
                <Div className="dos-frame sidebar__folder-picker">
                    <span className="dos-frame__title">Assign folder</span>
                    <Div className="dos-frame__body">
                        <FolderPicker
                            selected={assigningChat?.folder_id ?? null}
                            onSelect={async (folderId) => {
                                if (assigningChat) await setChatFolder(assigningChat.id, folderId)
                                setAssigningChat(null)
                            }}
                        />
                    </Div>
                </Div>
            </Popup>
        </Div>
    )
};
