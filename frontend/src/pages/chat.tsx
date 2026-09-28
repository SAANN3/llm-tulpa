import {Fragment, useEffect, useRef, useState} from 'react'
import {Navigate, useSearchParams} from 'react-router-dom'
import '../styles/chat.scss'
import type {ThinkChoice} from '../api/agent/types'
import {getChats} from '../api/chats/get'
import type {MessageSearchOut} from '../api/chats/types'
import {ChatHeader} from '../components/chat-header.tsx'
import {ChatMessage} from '../components/chat-message.tsx'
import {DateSeparator} from '../components/date-separator.tsx'
import type {LazyListHandle} from '../components/lazy-list.tsx'
import {LazyList} from '../components/lazy-list.tsx'
import {NoticeMessage} from '../components/notice-message.tsx'
import {PendingAssistantMessage} from '../components/pending-assistant-message.tsx'
import {Button, Div, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {ToolConfirmation} from '../components/tool-confirmation.tsx'
import {ToolMessage} from '../components/tool-message.tsx'
import {UserInput} from '../components/user-input.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import type {Decisions, PendingConfirmations, TurnResult} from '../hooks/use-messages.ts'
import {ToolAllowance, useMessages} from '../hooks/use-messages.ts'
import {useServerEvent} from '../hooks/use-server-events.ts'
import {getAutoConfirm} from '../utils/auto-confirm.ts'
import {consumePendingPrompt, peekPendingPrompt} from '../utils/pending-prompt.ts'
import {isSameDay} from '../utils/dates'

/** Auto-confirm's stand-in decision: grant the escalation, or acknowledge with nothing to grant */
const autoConfirmDecisions = (pending: PendingConfirmations): Decisions => {
    const decisions: Decisions = {}
    for (const key of Object.keys(pending)) {
        const index = Number(key)
        decisions[index] = pending[index].escalation ? ToolAllowance.Permanent : ToolAllowance.Forbid
    }
    return decisions
};

interface PausedTurn {
    pending: PendingConfirmations
    confirm: (decisions: Decisions) => Promise<TurnResult>
}

const LOAD_MORE_THRESHOLD = 80

/** Route guard for /chat: without a valid id there's nothing to render */
const Chat = () => {
    const [searchParams] = useSearchParams()
    const idParam = searchParams.get('id')
    const chatId = idParam == null ? NaN : Number(idParam)

    if (idParam == null || !Number.isInteger(chatId)) return <Navigate to="/" replace/>

    return <ChatView chatId={chatId}/>
};

const ChatView = ({chatId}: { chatId: number }) => {
    const lazyListRef = useRef<LazyListHandle>(null)
    const {messages, total, loadOlder, send, resume, runJobNotices, sending, canContinue, turnTokens} = useMessages(chatId, () =>
        lazyListRef.current?.jumpToBottom(),
    )

    const [chatName, setChatName] = useState<string | null>(null)
    const [chatModel, setChatModel] = useState<string | null>(null)
    const [chatProvider, setChatProvider] = useState('ollama')
    const [contextUsed, setContextUsed] = useState<number | null>(null)
    const [contextMax, setContextMax] = useState<number | null>(null)
    const [folderId, setFolderId] = useState<number | null>(null)
    useDocumentTitle(chatName ?? 'Chat')

    useEffect(() => {
        setChatName(null)
        setChatModel(null)
        setContextUsed(null)
        setContextMax(null)
        setFolderId(null)
        let cancelled = false

        getChats({id: chatId}).then((result) => {
            if (cancelled) return
            if (!('chats' in result)) {
                setChatName(result.name)
                setChatModel(result.model)
                setChatProvider(result.provider)
                setContextUsed(result.last_prompt_tokens)
                setContextMax(result.context_length)
                setFolderId(result.folder_id)
            }
        })

        return () => {
            cancelled = true
        }
    }, [chatId])

    const [expandedTools, setExpandedTools] = useState<Record<number, boolean>>({})
    const [expandedThinking, setExpandedThinking] = useState<Record<number, boolean>>({})
    const toggleToolExpanded = (id: number) =>
        setExpandedTools((prev) => ({...prev, [id]: !prev[id]}))
    const toggleThinkingExpanded = (id: number) =>
        setExpandedThinking((prev) => ({...prev, [id]: !prev[id]}))
    const [searchHighlight, setSearchHighlight] = useState<{
        messageId: number
        query: string
        matchedIn: string
    } | null>(null)
    const [pausedTurn, setPausedTurn] = useState<PausedTurn | null>(null)
    const [turnError, setTurnError] = useState<string | null>(null)
    const chatIdRef = useRef(chatId)
    chatIdRef.current = chatId

    // Shared by every collapsible message (thinking traces, tool output): keeps the
    // toggled element pinned on screen instead of `LazyList`'s default bottom-relative
    // scroll correction. An `if`/`else`, not `?? mutate()` — `preserveViewportPosition`
    // returns `void`, so `??` would read that as nullish and run `mutate` a second time,
    // silently cancelling the toggle it just performed.
    const preserveScrollFor = (anchor: HTMLElement, mutate: () => void) => {
        if (lazyListRef.current) lazyListRef.current.preserveViewportPosition(anchor, mutate)
        else mutate()
    }

    // Moves the context-usage bar during a turn, not just once it fully ends: a turn with
    // tool calls makes several Ollama calls in sequence, each with its own context usage,
    // but only the very last one reaches this component as a `TurnResult` (see
    // `handleTurnResult` below) — `turn_progress` is the same per-call event the pending
    // bubble already uses for its own live token count, just also carrying the running
    // context size.
    useServerEvent('turn_progress', (event) => {
        if (event.chat_id !== chatId || event.prompt_tokens == null) return
        setContextUsed(event.prompt_tokens + event.eval_tokens)
    })

    const handleTurnResult = (forChatId: number, result: TurnResult) => {
        if (chatIdRef.current !== forChatId) return
        // Each segment of a turn carries the context usage right after it — the last
        // segment's is the current one
        if (!result.needsConfirmation && result.reply.prompt_tokens != null) {
            setContextUsed(result.reply.prompt_tokens + (result.reply.eval_tokens ?? 0))
        }
        if (result.needsConfirmation && getAutoConfirm()) {
            setPausedTurn(null)
            result.confirm(autoConfirmDecisions(result.pending)).then(
                (next) => handleTurnResult(forChatId, next),
                () => {
                    if (chatIdRef.current === forChatId) setTurnError('Something went wrong continuing that turn — try again.')
                },
            )
            return
        }

        setPausedTurn(result.needsConfirmation ? {pending: result.pending, confirm: result.confirm} : null)
    }
    const handleConfirm = async (decisions: Decisions) => {
        if (!pausedTurn) return
        const {confirm} = pausedTurn
        const forChatId = chatId
        setPausedTurn(null)
        setTurnError(null)
        try {
            handleTurnResult(forChatId, await confirm(decisions))
        } catch {
            if (chatIdRef.current === forChatId) setTurnError("Something went wrong continuing that turn — try again.")
        }
    }

    // What the composer last asked for, reused for a turn the backend starts itself (a finished job)
    const lastThinkRef = useRef<ThinkChoice>(true)
    const handleSend = async (prompt: string, think?: ThinkChoice, images?: string[], fileIds?: number[]) => {
        const forChatId = chatId
        lastThinkRef.current = think ?? true
        setTurnError(null)
        setSearchHighlight(null)
        try {
            handleTurnResult(forChatId, await send(prompt, think, images, fileIds))
        } catch {
            if (chatIdRef.current === forChatId) setTurnError('Something went wrong sending that — try again.')
        }
    }

    const sendRef = useRef(handleSend)
    sendRef.current = handleSend
    const resumeRef = useRef(resume)
    resumeRef.current = resume
    const runJobNoticesRef = useRef(runJobNotices)
    runJobNoticesRef.current = runJobNotices
    const handleTurnResultRef = useRef(handleTurnResult)
    handleTurnResultRef.current = handleTurnResult
    const [initialThink] = useState(() => peekPendingPrompt(chatId)?.think !== false)

    useEffect(() => {
        setPausedTurn(null)
        setExpandedTools({})
        setExpandedThinking({})
        setTurnError(null)
        setSearchHighlight(null)
    }, [chatId])

    useEffect(() => {
        const pending = consumePendingPrompt(chatId)
        if (pending) sendRef.current(pending.prompt, pending.think, pending.images, pending.fileIds)
    }, [chatId])

    // A finished job is only a hint and can arrive mid-turn, so it's recorded here and acted on
    // once the chat is idle — deferred, never dropped
    const [noticesWaiting, setNoticesWaiting] = useState(false)
    useServerEvent('job_finished', (event) => {
        if (event.chat_id === chatIdRef.current) setNoticesWaiting(true)
    })

    useEffect(() => {
        if (!noticesWaiting || sending || pausedTurn) return
        setNoticesWaiting(false)
        const forChatId = chatId

        runJobNoticesRef
            .current(lastThinkRef.current)
            .then((result) => {
                if (result) handleTurnResultRef.current(forChatId, result)
            })
            .catch(() => {
                if (chatIdRef.current === forChatId) setTurnError('Something went wrong reacting to a finished job — try again.')
            })
    }, [noticesWaiting, sending, pausedTurn, chatId])

    useEffect(() => {
        if (!canContinue) return
        const forChatId = chatId

        resumeRef
            .current()
            .then((result) => {
                if (result) handleTurnResult(forChatId, result)
            })
            .catch(() => {
                if (chatIdRef.current === forChatId) setTurnError('Something went wrong resuming that turn — try again.')
            })
    }, [canContinue, chatId])

    // A search hit may live in a page that isn't loaded yet: load older pages until it's
    // mounted, then scroll to it.
    const messagesRef = useRef(messages)
    messagesRef.current = messages
    const totalRef = useRef(total)
    totalRef.current = total
    const loadOlderRef = useRef(loadOlder)
    loadOlderRef.current = loadOlder

    const jumpToMessage = async (hit: MessageSearchOut, query: string) => {
        const keyword = query.trim() || hit.matched
        setSearchHighlight({messageId: hit.id, query: keyword, matchedIn: hit.matched_in})
        setExpandedTools((prev) => ({...prev, [hit.id]: true}))
        setExpandedThinking((prev) => ({...prev, [hit.id]: true}))
        const messageId = String(hit.id)
        const settle = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))

        for (let attempt = 0; attempt < 500; attempt += 1) {
            await settle()
            if (lazyListRef.current?.scrollToMessage(messageId)) {
                await settle()
                lazyListRef.current?.scrollToMessage(messageId)
                return
            }
            const loaded = await loadOlderRef.current()
            if (!loaded && messagesRef.current.length >= totalRef.current) return
            await settle()
        }
    }

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="chat">
                <ChatHeader chatId={chatId} name={chatName} model={chatModel} provider={chatProvider}
                            onModelChanged={setChatModel}
                            contextUsed={contextUsed}
                            contextMax={contextMax}
                            folderId={folderId}
                            onFolderChanged={setFolderId}
                            onSelectSearchResult={jumpToMessage}
                            hasActiveHighlight={searchHighlight != null}
                            onClearHighlight={() => setSearchHighlight(null)}/>
                <LazyList ref={lazyListRef} className="chat__list" threshold={LOAD_MORE_THRESHOLD}
                          onTopReached={loadOlder}>
                    {messages.map((m, i) => {
                        const prev = messages[i - 1]
                        const showDate = !prev || !isSameDay(new Date(m.created_at), new Date(prev.created_at))
                        const isHit = m.id != null && searchHighlight?.messageId === m.id
                        const highlightQuery = isHit && searchHighlight ? searchHighlight.query : null
                        const id = m.id
                        return (
                            <Fragment key={m.id ?? i}>
                                {showDate ? <DateSeparator date={new Date(m.created_at)}/> : null}
                                <div className="chat__message" data-message-id={m.id}>
                                    {m.role === 'notice' ? (
                                        <NoticeMessage content={m.content}/>
                                    ) : m.role === 'tool' ? (
                                        <ToolMessage
                                            tool_name={m.tool_name ?? m.role}
                                            content={m.content}
                                            created_at={m.created_at}
                                            arguments={m.arguments}
                                            expanded={id != null ? (expandedTools[id] ?? false) : false}
                                            onToggle={() => id != null && toggleToolExpanded(id)}
                                            preserveScrollFor={preserveScrollFor}
                                            highlightQuery={highlightQuery}
                                        />
                                    ) : (
                                        <ChatMessage
                                            role={m.role}
                                            content={m.content}
                                            created_at={m.created_at}
                                            thinking={m.thinking}
                                            thought_duration_ms={m.thought_duration_ms}
                                            images={m.images}
                                            file_ids={m.file_ids}
                                            eval_tokens={m.eval_tokens}
                                            highlightQuery={highlightQuery}
                                            thinkingExpanded={id != null ? (expandedThinking[id] ?? false) : false}
                                            onToggleThinking={() => id != null && toggleThinkingExpanded(id)}
                                            preserveScrollFor={preserveScrollFor}
                                        />
                                    )}
                                </div>
                            </Fragment>
                        )
                    })}
                    {sending ? <PendingAssistantMessage tokens={turnTokens}/> : null}
                </LazyList>
                {pausedTurn ? <ToolConfirmation pending={pausedTurn.pending} onConfirm={handleConfirm}/> : null}
                {turnError ? (
                    <Div className="chat__error">
                        <Label className="chat__error-text" text={turnError}/>
                        <Button variant="secondary" text="Dismiss" onClicked={() => setTurnError(null)}/>
                    </Div>
                ) : null}
                <UserInput
                    blocked={sending || pausedTurn != null}
                    onSended={handleSend}
                    inputDisabled={false}
                    initialThink={initialThink}
                    chatId={chatId}
                    model={chatModel}
                />
            </Div>
        </Div>
    )
};

export default Chat
