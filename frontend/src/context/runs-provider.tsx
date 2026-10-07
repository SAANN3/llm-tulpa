import {useCallback, useEffect, useMemo, useRef, useState, type ReactNode} from 'react'
import {useLocation} from 'react-router-dom'
import {RunsContext} from './runs-context.ts'
import {useAuth} from './use-auth.ts'
import {useSettings} from './use-settings.ts'
import {getRuns} from '../api/agent/runs'
import {getTurnState} from '../api/agent/turn-state'
import type {AgentToolCall, RunEnded} from '../api/agent/types'
import {getMessages} from '../api/chats/messages'
import {setChatSeen} from '../api/chats/set-seen'
import {useServerEvent} from '../hooks/use-server-events.ts'
import {notify} from '../utils/notifications'

const NOTIFICATION_BODY_MAX_CHARS = 100

const truncateForNotification = (text: string): string => {
    const trimmed = text.trim()
    if (trimmed.length <= NOTIFICATION_BODY_MAX_CHARS) return trimmed

    return `${trimmed.slice(0, NOTIFICATION_BODY_MAX_CHARS).trimEnd()}...`
};

/** The calls of a wait for permission that need an answer, by tool name */
const askingToolNames = (pending: AgentToolCall[]): string =>
    pending
        .filter((call) => call.permission.status === 'denied' && call.permission.escalation)
        .map((call) => call.name)
        .join(', ')

/** The newest assistant message's text, for a notification */
const lastReplyText = async (chatId: number): Promise<string> => {
    const {messages} = await getMessages({chatId, limit: 10})
    const reply = messages.find((m) => m.role === 'assistant' && m.content.trim().length > 0)
    return reply?.content ?? ''
};

/** A chat with a run going on, and the chat that started it when it is a sub-agent's */
interface WorkingChat {
    chatId: number
    parentChatId: number | null
}

/**
 * Follows every chat's run for the whole app, whatever page is open: which chats work (for the sidebar), marking the
 * open chat's runs as seen, and the browser notification when one ends. The backend decides all of it; this only
 * listens to its events and asks again when it could have missed some. How a chat's last run ended is the chat
 * list's own (`useChats`).
 */
export const RunsProvider = ({children}: { children: ReactNode }) => {
    const {token} = useAuth()
    const {settings} = useSettings()
    const settingsRef = useRef(settings)
    settingsRef.current = settings

    const [runs, setRuns] = useState<WorkingChat[]>([])

    // The chat page on screen, if any: its runs end as seen
    const {pathname, search} = useLocation()
    const idParam = new URLSearchParams(search).get('id')
    const openChatId = pathname === '/chat' && idParam != null && Number.isInteger(Number(idParam)) ? Number(idParam) : null
    const openChatIdRef = useRef(openChatId)
    openChatIdRef.current = openChatId

    // Bumped by every event: a read that was started before an event and answered after it is out of date and must
    // not put back what the event just replaced (a run that ended shown as working again)
    const eventSeqRef = useRef(0)

    /** Tells the backend the user has looked at the chat; it tells the other pages. Nothing to do if it fails */
    const markSeen = useCallback((chatId: number) => {
        setChatSeen(chatId).catch(() => undefined)
    }, [])

    /** Reads which chats run. `retry`: an event came while the read was on its way, so what it says may be old */
    const refreshRuns = useCallback(async (retry = true) => {
        const seq = eventSeqRef.current
        try {
            const list = await getRuns()
            if (eventSeqRef.current !== seq) {
                if (retry) setTimeout(() => void refreshRuns(false), 300)
                return
            }
            setRuns(list.map((run) => ({chatId: run.chat_id, parentChatId: run.parent_chat_id})))
        } catch {
            // The next event or refresh tries again
        }
    }, [])

    useEffect(() => {
        if (token) void refreshRuns()
        else setRuns([])
    }, [token, refreshRuns])

    // Opening a chat, or coming back to its tab, is looking at it
    useEffect(() => {
        if (token && openChatId != null && !document.hidden) markSeen(openChatId)
    }, [token, openChatId, markSeen])

    // A background tab's connection can miss events: what happened meanwhile is read once it is shown again
    useEffect(() => {
        const onVisible = () => {
            if (document.visibilityState !== 'visible' || !token) return
            void refreshRuns()
            if (openChatIdRef.current != null) markSeen(openChatIdRef.current)
        }
        document.addEventListener('visibilitychange', onVisible)
        return () => document.removeEventListener('visibilitychange', onVisible)
    }, [token, refreshRuns, markSeen])

    // The stream (re)opened: events sent while it was down are gone
    useServerEvent('stream_open', () => void refreshRuns())

    useServerEvent('run_started', (event) => {
        eventSeqRef.current += 1
        setRuns((prev) => [...prev.filter((run) => run.chatId !== event.chat_id), {chatId: event.chat_id, parentChatId: event.parent_chat_id}])
    })

    useServerEvent('run_ended', (event) => {
        eventSeqRef.current += 1
        setRuns((prev) => prev.filter((run) => run.chatId !== event.chat_id))
        if (openChatIdRef.current === event.chat_id && !document.hidden) markSeen(event.chat_id)
        void notifyEnd(event.chat_id, event)
    })

    /** The browser notification for a run that ended: a reply, or a question waiting for the user */
    const notifyEnd = async (chatId: number, event: Pick<RunEnded, 'reason' | 'started_at'>) => {
        const current = settingsRef.current
        if (!current?.notifications_enabled) return
        const tag = `run-${chatId}-${event.started_at}`
        try {
            if (event.reason === 'waiting_for_permission') {
                if (current.auto_confirm) return
                const {pending} = await getTurnState(chatId)
                await notify('llm-tulpa', `Waiting on your OK to run: ${askingToolNames(pending)}`, tag)
            } else if (event.reason === 'answered' || event.reason === 'step_limit') {
                const text = await lastReplyText(chatId)
                if (text.trim().length > 0) await notify('llm-tulpa', truncateForNotification(text), tag)
            }
        } catch {
            // A notification is a convenience: nothing to do if what it needs can't be read
        }
    }

    const value = useMemo(() => {
        const working = new Set<number>()
        for (const run of runs) {
            working.add(run.chatId)
            if (run.parentChatId != null) working.add(run.parentChatId)
        }
        return {working}
    }, [runs])

    return <RunsContext.Provider value={value}>{children}</RunsContext.Provider>
};
