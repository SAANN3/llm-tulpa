import {useEffect, useRef, useState} from 'react'
import {ArrowBarLeft, ArrowBarRight, Plus, Folder, Search, Check, Lock, WarningDiamond} from 'pixelarticons/react'
import {useLocation, useNavigate, useSearchParams} from 'react-router-dom'
import '../styles/sidebar.scss'
import {AccountMenu} from './account-menu.tsx'
import {ChatEntry} from './chat-entry.tsx'
import type {LazyListHandle} from './lazy-list.tsx'
import {LazyList} from './lazy-list.tsx'
import {AssignFolderPopup} from './popups/assign-folder-popup.tsx'
import {Button, Div, Label, Link} from './primitives'
import {ThinkingAnimation} from './thinking-animation.tsx'
import {setChatFolder} from '../api/chats/set-folder'
import type {ChatOut, UnseenEnd} from '../api/chats/types'
import {useRuns} from '../context/use-runs.ts'
import {onSidebarWindowResized, setSidebarCollapsed, useSidebarCollapsed} from '../context/sidebar-state.ts'
import {useChats} from '../hooks/use-chats.ts'
import {daysBefore} from '../utils/dates'
import {isMobileViewport} from '../utils/viewport.ts'

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

interface UtilItem {
    label: string
    path: string
    icon: (size: number) => React.ReactNode
    match?: (pathname: string) => boolean
}

const UTILS: UtilItem[] = [
    {
        label: 'Search chats',
        path: '/search',
        icon: (size) => <Search width={size} height={size}/>,
        match: (pathname) => pathname === '/search',
    },
    {
        label: 'Folders',
        path: '/folders',
        icon: (size) => <Folder width={size} height={size}/>,
        match: (pathname) => pathname.startsWith('/folders'),
    },
]

/** The chat-list sidebar, shared between the chat and home routes */
export const Sidebar = () => {
    const navigate = useNavigate()
    const location = useLocation()
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

    // Auto-collapse sidebar on mobile only when navigating to another route
    const lastLocationKey = useRef(location.pathname + location.search)
    useEffect(() => {
        const currentKey = location.pathname + location.search
        if (currentKey !== lastLocationKey.current) {
            lastLocationKey.current = currentKey
            if (isMobileViewport()) {
                setSidebarCollapsed(true)
            }
        }
    }, [location.pathname, location.search])

    const handleSidebarClick = (e: React.MouseEvent) => {
        if (isMobileViewport()) {
            const target = e.target as HTMLElement
            if (target.closest('a, .chat-entry__link') && !target.closest('.sidebar__toggle, .account-menu__anchor')) {
                setSidebarCollapsed(true)
            }
        }
    }

    const toggle = (
        <Button
            className="sidebar__toggle"
            title={collapsed ? 'Expand sidebar' : 'Shrink sidebar'}
            onClicked={() => setSidebarCollapsed(!collapsed)}
        >
            {collapsed ? <ArrowBarRight width={18} height={18}/> : <ArrowBarLeft width={18} height={18}/>}
        </Button>
    )

    // The rail's icons: what the full sidebar lists above the chats; the rest is in the account menu at the bottom
    const utilsCollapsed = [
        {label: 'New chat', path: '/', icon: <Plus width={16} height={16}/>},
        ...UTILS.map((item) => ({
            label: item.label,
            path: item.path,
            icon: item.icon(16),
        })),
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
                        <Link key={item.label} to={item.path} variant="primary" title={item.label}
                              className="link-button sidebar__utils-item sidebar__utils-item--compact">
                            {item.icon}
                        </Link>
                    ))}
                </Div>
                <Div className="sidebar__account">
                    <AccountMenu compact/>
                </Div>
            </Div>
        )
    }

    return (
        <>
            <div
                className="sidebar__backdrop"
                onClick={() => setSidebarCollapsed(true)}
            />
            <div className="sidebar__top-placeholder" />
            <Div className="vbox sidebar" onClick={handleSidebarClick}>
                <Div className="sidebar__top">
                    {toggle}
                    <Link to="/" variant="primary" className="link-button sidebar__new-chat">New chat</Link>
                </Div>
                <Label className="section-heading" text="Utils"/>
                <Div className="vbox">
                    {UTILS.map((item) => (
                        <ChatEntry
                            key={item.path}
                            label={item.label}
                            selected={item.match ? item.match(location.pathname) : location.pathname === item.path}
                            icon={item.icon(18)}
                            href={item.path}
                        />
                    ))}
                </Div>
                {/* "Chats" heads the first date group itself ("Chats · Today"), so the list doesn't open with two titles in a
                    row; with no chats yet it stands alone */}
                {chats.length === 0 ? <Label className="section-heading" text="Chats"/> : null}
                <LazyList ref={chatsListRef} onBottomReached={loadOlder} className="sidebar__list">
                    <Div className="vbox sidebar__groups">
                        {groupByRecency(chats).map(([group, groupChats], i) => (
                            <Div className="vbox sidebar__group" key={group}>
                                <Label className="section-heading sidebar__group-heading" text={i === 0 ? `Chats · ${group}` : group}/>
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
                <Div className="sidebar__account">
                    <AccountMenu/>
                </Div>
            </Div>
        </>
    )
};
