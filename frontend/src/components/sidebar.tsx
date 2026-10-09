import {useEffect, useRef, useState} from 'react'
import {ArrowBarLeft, ArrowBarRight, Plus, Settings2, User, Folder, Plug, Logout, Analytics, Search, Cpu, Check, Lock, WarningDiamond} from 'pixelarticons/react'
import {useLocation, useNavigate, useSearchParams} from 'react-router-dom'
import '../styles/sidebar.scss'
import {ChatEntry} from './chat-entry.tsx'
import type {LazyListHandle} from './lazy-list.tsx'
import {LazyList} from './lazy-list.tsx'
import {AssignFolderPopup} from './popups/assign-folder-popup.tsx'
import {ConfirmPopup} from './popups/base/confirm-popup.tsx'
import {Button, Div, Label, Link} from './primitives'
import {ThinkingAnimation} from './thinking-animation.tsx'
import {setChatFolder} from '../api/chats/set-folder'
import type {ChatOut, UnseenEnd} from '../api/chats/types'
import {useAuth} from '../context/use-auth.ts'
import {useRuns} from '../context/use-runs.ts'
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

/** What a chat's row shows about its run: a small animation while it works, else how the last run ended until the chat is opened */
const runIcon = (working: boolean, unseenEnd: UnseenEnd | null) => {
    if (working) return <ThinkingAnimation className="chat-entry__running" isPlaying/>
    if (unseenEnd === 'answered' || unseenEnd === 'step_limit') return <Check width={16} height={16}/>
    if (unseenEnd === 'waiting_for_permission') return <Lock width={16} height={16}/>
    if (unseenEnd === 'failed') return <WarningDiamond width={16} height={16}/>
    return undefined
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
    const [confirmingLogout, setConfirmingLogout] = useState(false)
    // In both layouts of the sidebar, so the rail's button asks too
    const logoutPopup = (
        <ConfirmPopup open={confirmingLogout} title="Log out" confirmLabel="Log out"
                      message="Log out of this browser? Your chats stay as they are." onConfirm={logout}
                      onClose={() => setConfirmingLogout(false)}/>
    )
    const [searchParams] = useSearchParams()
    const idParam = searchParams.get('id')
    const selectedChatId = idParam == null ? null : Number(idParam)

    const chatsListRef = useRef<LazyListHandle>(null)
    const {chats, loadOlder, rename, delete: deleteChat} = useChats(() => chatsListRef.current?.jumpToTop())
    const {working} = useRuns()
    const [assigningChat, setAssigningChat] = useState<ChatOut | null>(null)
    const collapsed = useSidebarCollapsed()

    // The only resize caller: a crossing of the threshold collapses/expands, a resize that
    // stays on one side leaves a manual choice alone (see onSidebarWindowResized)
    useEffect(() => {
        const onResize = () => onSidebarWindowResized(window.innerWidth)
        window.addEventListener('resize', onResize)
        return () => window.removeEventListener('resize', onResize)
    }, [])



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
        {label: 'New chat', path: '/', icon: <Plus width={16} height={16}/>},
        {label: 'Search chats', path: '/search', icon: <Search width={16} height={16}/>},
        {label: 'Settings', path: '/settings', icon: <Settings2 width={16} height={16}/>},
        {label: 'Folders', path: '/folders', icon: <Folder width={16} height={16}/>},
        {label: 'Usage', path: '/stats', icon: <Analytics width={16} height={16}/>},
        {label: 'Models', path: '/models', icon: <Cpu width={16} height={16}/>},
        ...(user?.role === 'owner' ? [
            {label: 'Plugins', path: '/plugins', icon: <Plug width={16} height={16}/>},
            {label: 'Users', path: '/users', icon: <User width={16} height={16}/>},
        ] : []),
        {label: 'Log out', path: null, icon: <Logout width={16} height={16}/>, action: () => setConfirmingLogout(true)},
    ]

    // The shrunk rail: toggle + utils icons, no text
    if (collapsed) {
        return (
            <Div className="vbox sidebar sidebar--collapsed">
                <Div className="sidebar__top">
                    {toggle}
                </Div>
                <Div className="sidebar__utils-collapsed">
                    {utilsCollapsed.map(item => item.path != null ? (
                        <Link key={item.label} to={item.path} variant="primary" title={item.label}
                              className="link-button sidebar__utils-item sidebar__utils-item--compact">
                            {item.icon}
                        </Link>
                    ) : (
                        <Button key={item.label} className="sidebar__utils-item sidebar__utils-item--compact" title={item.label}
                                onClicked={item.action}>
                            {item.icon}
                        </Button>
                    ))}
                </Div>
                {logoutPopup}
            </Div>
        )
    }

    return (
        <Div className="vbox sidebar">
            <Div className="sidebar__top">
                {toggle}
                <Link to="/" variant="primary" className="link-button sidebar__new-chat">New chat</Link>
            </Div>
            <Label className="section-heading" text="Utils"/>
            <Div className="vbox">
                <ChatEntry label="Search chats" selected={location.pathname === '/search'} icon={<Search width={18} height={18}/>} href="/search"/>
                <ChatEntry label="Settings" selected={location.pathname.startsWith('/settings')} icon={<Settings2 width={18} height={18}/>} href="/settings"/>
                <ChatEntry label="Folders" selected={location.pathname === '/folders'} icon={<Folder width={18} height={18}/>} href="/folders"/>
                <ChatEntry label="Usage" selected={location.pathname.startsWith('/stats')} icon={<Analytics width={18} height={18}/>} href="/stats"/>
                <ChatEntry label="Models" selected={location.pathname.startsWith('/models')} icon={<Cpu width={18} height={18}/>} href="/models"/>
                {user?.role === 'owner' ? (
                    <>
                        <ChatEntry label="Plugins" selected={location.pathname === '/plugins'} icon={<Plug width={18} height={18}/>} href="/plugins"/>
                        <ChatEntry label="Users" selected={location.pathname === '/users'} icon={<User width={18} height={18}/>} href="/users"/>
                    </>
                ) : null}
                <ChatEntry label="Log out" selected={false} icon={<Logout width={18} height={18}/>} onClicked={() => setConfirmingLogout(true)}/>
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
                                    href={`/chat?id=${c.id}`}
                                    onRename={(name) => rename(c.id, name)}
                                    onDelete={() => {
                                        deleteChat(c.id).then(() => {
                                            if (location.pathname === '/chat' && selectedChatId === c.id) navigate('/')
                                        })
                                    }}
                                    onAssignFolder={() => setAssigningChat(c)}
                                    icon={runIcon(working.has(c.id), c.id === selectedChatId && !document.hidden ? null : c.unseen_end)}
                                />
                            ))}
                        </Div>
                    ))}
                </Div>
            </LazyList>
            <AssignFolderPopup
                open={assigningChat != null}
                selected={assigningChat?.folder_id ?? null}
                onSelect={async (folderId) => {
                    if (assigningChat) await setChatFolder(assigningChat.id, folderId)
                    setAssigningChat(null)
                }}
                onClose={() => setAssigningChat(null)}
            />
            {logoutPopup}
        </Div>
    )
};
