import {useEffect, useRef, useState} from 'react'
import {rewindChat} from '../api/chats/rewind.ts'
import {getMessages} from '../api/chats/messages'
import type {MessageOut} from '../api/chats/types'
import {peekPendingPrompt} from '../utils/pending-prompt.ts'
import {useServerEvent} from './use-server-events.ts'

const MESSAGES_PAGE_SIZE = 30

export type DisplayMessage =
    | {
    id?: number
    role: 'user' | 'assistant'
    content: string
    created_at: string
    thinking?: string | null
    thought_duration_ms?: number | null
    images?: string[]
    file_ids?: number[]
    prompt_tokens?: number | null
    eval_tokens?: number | null
}
    | {
    id?: number
    role: 'tool';
    content: unknown;
    tool_name: string | null;
    created_at: string;
    arguments: Record<string, unknown>
}
    | { id?: number; role: 'notice'; content: string; created_at: string }

/**
 * The arguments of the tool calls a page of messages made, handed to the tool messages that answer them
 * in order. Kept between pages: a reply's calls can be in one fetch and their results in the next.
 */
interface ToolArgsQueue {
    args: Record<string, unknown>[]
}

/** Maps a fetched page of messages (oldest first) to display form, pairing each tool message with its call arguments */
const toDisplayMessages = (page: MessageOut[], queue: ToolArgsQueue = {args: []}): DisplayMessage[] => {
    return page.map((m) => {
        if (m.role === 'assistant' && m.tool_calls.length > 0) {
            queue.args = m.tool_calls.map((call) => call.arguments)
        }
        if (m.role === 'tool') {
            return {
                id: m.id,
                role: 'tool',
                content: m.content,
                tool_name: m.tool_name,
                created_at: m.created_at,
                arguments: queue.args.shift() ?? {},
            }
        }
        if (m.role === 'notice') {
            return {id: m.id, role: 'notice', content: m.content, created_at: m.created_at}
        }
        return {
            id: m.id,
            role: m.role as 'user' | 'assistant',
            content: m.content,
            created_at: m.created_at,
            thinking: m.thinking,
            thought_duration_ms: m.thought_duration_ms,
            images: m.images,
            file_ids: m.file_ids,
            prompt_tokens: m.prompt_tokens ?? null,
            eval_tokens: m.eval_tokens ?? null,
        }
    })
};

/** The newest stored message's id, 0 for a chat with none: what `fetchNew` asks for the messages after */
const lastStoredId = (list: DisplayMessage[]): number =>
    list.reduce((max, m) => (m.id != null && m.id > max ? m.id : max), 0)

/** A chat's message timeline: pages of it, and the messages the backend stores while a run goes on */
export const useMessages = (chatId: number, onAppended?: () => void) => {
    const [messages, setMessages] = useState<DisplayMessage[]>([])
    const [total, setTotal] = useState(0)
    // The chat whose first page `messages` holds: until it is this chat's, `messages` is still the
    // previous chat's (it is replaced when the fetch returns, not cleared on the switch)
    const [loadedChatId, setLoadedChatId] = useState<number | null>(null)
    const onAppendedRef = useRef(onAppended)
    onAppendedRef.current = onAppended

    const chatIdRef = useRef(chatId)
    chatIdRef.current = chatId
    const trackedChatIdRef = useRef<number | null>(null)
    const skipInitialFetchRef = useRef(false)
    if (trackedChatIdRef.current !== chatId) {
        trackedChatIdRef.current = chatId
        skipInitialFetchRef.current = peekPendingPrompt(chatId) != null
    }

    // `loadOlder` is called from the list's top-reached event and from a search jump's own loop,
    // each holding the closure of the render it was created in: the in-flight flag and the page
    // offset have to be read from refs, or two calls fetch the same page and prepend it twice.
    const loadingMoreRef = useRef(false)
    const messagesRef = useRef(messages)
    messagesRef.current = messages
    const totalRef = useRef(total)
    totalRef.current = total
    const queueRef = useRef<ToolArgsQueue>({args: []})
    const readyRef = useRef(false)
    const wantNewRef = useRef(false)
    const fetchingNewRef = useRef(false)
    const fetchAgainRef = useRef(false)
    // Messages taken off the screen that the backend still holds for a moment (a reply being regenerated): `fetchNew` must not bring them back
    const hiddenIdsRef = useRef(new Set<number>())
    // Reads the chat's first page again: set while that read failed (the backend was going away at that moment)
    const retryFirstLoadRef = useRef<(() => void) | null>(null)

    /** Reads the messages newer than the last one shown. Called when the backend says it stored some; calls that arrive while one is in flight make it look once more */
    const fetchNew = async () => {
        const forChatId = chatIdRef.current
        // The chat's first page isn't here yet: it is read newest-first and would show these twice
        if (!readyRef.current) {
            wantNewRef.current = true
            return
        }
        if (fetchingNewRef.current) {
            fetchAgainRef.current = true
            return
        }
        fetchingNewRef.current = true
        try {
            do {
                fetchAgainRef.current = false
                const result = await getMessages({chatId: forChatId, afterId: lastStoredId(messagesRef.current)})
                if (chatIdRef.current !== forChatId) return
                const known = new Set(messagesRef.current.map((m) => m.id))
                const added = toDisplayMessages([...result.messages].reverse(), queueRef.current)
                    .filter((m) => !known.has(m.id) && !(m.id != null && hiddenIdsRef.current.has(m.id)))
                totalRef.current = result.total
                setTotal(result.total)
                if (added.length > 0) {
                    messagesRef.current = [...messagesRef.current, ...added]
                    setMessages(messagesRef.current)
                    onAppendedRef.current?.()
                }
            } while (fetchAgainRef.current)
        } finally {
            fetchingNewRef.current = false
        }
    }
    const fetchNewRef = useRef(fetchNew)
    fetchNewRef.current = fetchNew

    /** Reads the chat's newest page again, replacing what is shown: for when something stored was removed behind the page's back */
    const reload = async () => {
        const forChatId = chatIdRef.current
        const result = await getMessages({chatId: forChatId, limit: MESSAGES_PAGE_SIZE})
        if (chatIdRef.current !== forChatId) return
        const queue: ToolArgsQueue = {args: []}
        const shown = toDisplayMessages([...result.messages].reverse(), queue)
        queueRef.current = queue
        hiddenIdsRef.current.clear()
        messagesRef.current = shown
        totalRef.current = result.total
        setMessages(shown)
        setTotal(result.total)
    }

    useEffect(() => {
        let cancelled = false
        readyRef.current = false
        retryFirstLoadRef.current = null

        if (!skipInitialFetchRef.current) {
            const load = () => getMessages({chatId, limit: MESSAGES_PAGE_SIZE}).then((result) => {
                if (cancelled) return
                const queue: ToolArgsQueue = {args: []}
                const historical = toDisplayMessages([...result.messages].reverse(), queue)
                queueRef.current = queue
                messagesRef.current = historical
                totalRef.current = result.total
                setMessages(historical)
                setTotal(result.total)
                setLoadedChatId(chatId)
                readyRef.current = true
                onAppendedRef.current?.()
                // What was stored while the page was being read
                if (wantNewRef.current) {
                    wantNewRef.current = false
                    void fetchNewRef.current()
                }
            }, () => {
                // Read again when the event stream is back, which is when the backend is (see `stream_open` below)
                if (!cancelled) retryFirstLoadRef.current = () => void load()
            })
            void load()
        } else {
            // A chat just made for the home page's prompt: nothing in it yet, its messages arrive as they are stored
            queueRef.current = {args: []}
            messagesRef.current = []
            totalRef.current = 0
            setMessages([])
            setTotal(0)
            setLoadedChatId(chatId)
            readyRef.current = true
        }

        return () => {
            cancelled = true
            wantNewRef.current = false
        }
    }, [chatId])

    // The stream (re)opened, so the backend answers again: a first page that couldn't be read is read now
    useServerEvent('stream_open', () => {
        const retry = retryFirstLoadRef.current
        retryFirstLoadRef.current = null
        retry?.()
    })

    // A rewind or a regenerate somewhere else (another tab, another device) took these out of the chat. Here they may
    // be gone already (this page did it), and then nothing changes.
    useServerEvent('messages_removed', (event) => {
        if (event.chat_id !== chatIdRef.current) return
        const gone = new Set(event.message_ids)
        event.message_ids.forEach((id) => hiddenIdsRef.current.add(id))
        const kept = messagesRef.current.filter((m) => m.id == null || !gone.has(m.id))
        const removedHere = messagesRef.current.length - kept.length
        if (removedHere === 0) return
        messagesRef.current = kept
        totalRef.current -= removedHere
        setMessages(kept)
        setTotal((t) => t - removedHere)
    })

    const loadOlder = async (): Promise<boolean> => {
        if (loadingMoreRef.current || messagesRef.current.length >= totalRef.current) return false

        loadingMoreRef.current = true
        try {
            const skip = messagesRef.current.length
            const result = await getMessages({chatId, skip, limit: MESSAGES_PAGE_SIZE})
            const older = toDisplayMessages([...result.messages].reverse())
            // Bumped now, not at the next render: a caller that awaits this and calls again
            // straight away must not read the old offset.
            messagesRef.current = [...older, ...messagesRef.current]
            totalRef.current = result.total
            setMessages(messagesRef.current)
            setTotal(result.total)
            return older.length > 0
        } finally {
            loadingMoreRef.current = false
        }
    }

    /** Takes a message off the screen (not out of the chat): a reply the backend replaces once its new one is stored */
    const hideMessage = (messageId: number) => {
        hiddenIdsRef.current.add(messageId)
        messagesRef.current = messagesRef.current.filter((m) => m.id !== messageId)
        totalRef.current -= 1
        setMessages(messagesRef.current)
        setTotal((t) => t - 1)
    }

    /** Removes a message and everything after it, in the backend and on screen. Throws (with nothing removed) when the backend refuses, e.g. because a tool was used from there on. */
    const rewind = async (messageId: number) => {
        const requestChatId = chatId
        const deleted = await rewindChat(chatId, messageId)
        if (chatIdRef.current !== requestChatId) return

        const start = messagesRef.current.findIndex((m) => m.id === messageId)
        if (start < 0) return
        messagesRef.current = messagesRef.current.slice(0, start)
        totalRef.current -= deleted
        setMessages(messagesRef.current)
        setTotal((t) => t - deleted)
    }

    return {messages, total, ready: loadedChatId === chatId, loadOlder, fetchNew, reload, hideMessage, rewind}
};
