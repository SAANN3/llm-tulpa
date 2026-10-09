import axios from 'axios'
import {Fragment, useEffect, useRef, useState} from 'react'
import {Navigate, useLocation, useNavigate, useSearchParams} from 'react-router-dom'
import '../styles/chat.scss'
import type {ThinkChoice} from '../api/agent/types'
import {getChats} from '../api/chats/get'
// Named apart from the page's own setChatModel state setter
import {setChatModel as saveChatModel} from '../api/chats/set-model'
import {setChatProfile} from '../api/chats/set-profile'
import type {MessageSearchOut} from '../api/chats/types'
import {ChatHeader} from '../components/chat-header.tsx'
import {ChatMessage} from '../components/chat-message.tsx'
import {DateSeparator} from '../components/date-separator.tsx'
import type {LazyListHandle} from '../components/lazy-list.tsx'
import {LazyList} from '../components/lazy-list.tsx'
import {ModelStateBanner} from '../components/model-state-banner.tsx'
import {ConfirmPopup} from '../components/popups/base/confirm-popup.tsx'
import {ModelBusyPopup} from '../components/popups/model-busy-popup.tsx'
import {NoticeMessage} from '../components/notice-message.tsx'
import {Button, Div, Label} from '../components/primitives'
import {RunStatus} from '../components/run-status.tsx'
import {Sidebar} from '../components/sidebar.tsx'
import {ToolConfirmation} from '../components/tool-confirmation.tsx'
import {ToolMessage} from '../components/tool-message.tsx'
import {UserInput} from '../components/user-input.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useMessages} from '../hooks/use-messages.ts'
import type {TurnView} from '../hooks/use-turn.ts'
import {useTurn} from '../hooks/use-turn.ts'
import {useServerEvent} from '../hooks/use-server-events.ts'
import {useSettings} from '../context/use-settings.ts'
import {consumePendingPrompt, peekPendingPrompt} from '../utils/pending-prompt.ts'
import {isSameDay} from '../utils/dates'
import type {ModelChoice} from '../utils/model-choice.ts'
import {errorReason} from '../utils/error-reason.ts'
import {describeRunEnd} from '../utils/run-end.ts'
import type {Decision, RunEnded} from '../api/agent/types'

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
    const {settings} = useSettings()
    const lazyListRef = useRef<LazyListHandle>(null)
    const {messages, total, ready, loadOlder, fetchNew, reload, hideMessage, rewind} = useMessages(chatId, () =>
        lazyListRef.current?.jumpToBottom(),
    )
    const messagesRef = useRef(messages)
    messagesRef.current = messages
    const fetchNewRef = useRef(fetchNew)
    fetchNewRef.current = fetchNew
    const runEndedRef = useRef<(end: RunEnded, view: TurnView) => void>(() => undefined)
    const {view: turn, send, regenerate, answer, stop} = useTurn(chatId, {
        onMessagesChanged: () => void fetchNewRef.current(),
        onRunEnded: (end, view) => runEndedRef.current(end, view),
    })
    // A run is going on, or waits for the user: the composer and the edit controls are off
    const busy = turn.status !== 'idle'
    // How the last run ended, for one that ended without an answer (an answered run has nothing to say)
    const endLine = !busy && turn.lastEnd ? describeRunEnd(turn.lastEnd, settings?.max_turn_steps ?? null) : null

    const [chatName, setChatName] = useState<string | null>(null)
    const [chatModel, setChatModel] = useState<string | null>(null)
    const [chatProvider, setChatProvider] = useState('ollama')
    const [chatProfileId, setChatProfileId] = useState<number | null>(null)
    const [chatToolsEnabled, setChatToolsEnabled] = useState(true)
    const [contextUsed, setContextUsed] = useState<number | null>(null)
    const [contextMax, setContextMax] = useState<number | null>(null)
    const [folderId, setFolderId] = useState<number | null>(null)
    // undefined until the chat has been fetched: whether it is a sub-agent's chat changes what the page does
    const [parentChatId, setParentChatId] = useState<number | null | undefined>(undefined)
    useDocumentTitle(chatName ?? 'Chat')

    // A profile carries its model; an Ollama model moves the chat off any launch profile
    const changeModel = async (choice: ModelChoice) => {
        await saveChatModel(chatId, choice.model, choice.provider)
        if (choice.profileId != null) await setChatProfile(chatId, choice.profileId)
        setChatModel(choice.model)
        setChatProvider(choice.provider)
        setChatProfileId(choice.profileId)
        // A profile's context window is its own: the gauge's maximum follows the chat
        getChats({id: chatId}).then((result) => {
            if (!('chats' in result)) setContextMax(result.context_length)
        })
    }

    useEffect(() => {
        setChatName(null)
        setChatModel(null)
        setContextUsed(null)
        setContextMax(null)
        setFolderId(null)
        setParentChatId(undefined)
        let cancelled = false

        getChats({id: chatId}).then((result) => {
            if (cancelled) return
            if (!('chats' in result)) {
                setChatName(result.name)
                setChatModel(result.model)
                setChatProvider(result.provider)
                setChatProfileId(result.launch_profile_id)
                setChatToolsEnabled(result.tools_enabled)
                setContextUsed(result.last_prompt_tokens)
                setContextMax(result.context_length)
                setFolderId(result.folder_id)
                setParentChatId(result.parent_chat_id)
            }
        })

        return () => {
            cancelled = true
        }
    }, [chatId])

    // A rename from the header here, or from the sidebar or another page, reaches the title the same way
    useServerEvent('chat_renamed', (event) => {
        if (event.chat_id === chatId) setChatName(event.name)
    })

    // With an automatic context the window is whatever the server picked when it loaded the model, so
    // the gauge's maximum is read again once a load finishes.
    useServerEvent('model_state', (event) => {
        if (event.state !== 'ready') return
        getChats({id: chatId}).then((result) => {
            if (!('chats' in result)) setContextMax(result.context_length)
        })
    })

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
    const [turnError, setTurnError] = useState<string | null>(null)
    // Set when another user has the model server busy with a different model: that is a wait, not an error
    const [busyReason, setBusyReason] = useState<string | null>(null)
    const chatIdRef = useRef(chatId)
    chatIdRef.current = chatId

    /** Reports why a turn failed: a busy model server (423) gets its own popup, anything else the error bar */
    const turnFailed = (forChatId: number, e: unknown, fallback: string) => {
        if (chatIdRef.current !== forChatId) return
        if (axios.isAxiosError(e) && e.response?.status === 423) setBusyReason(errorReason(e, 'The model is in use by someone else.'))
        else setTurnError(errorReason(e, fallback))
    }

    // Shared by every collapsible message (thinking traces, tool output): keeps the
    // toggled element pinned on screen instead of `LazyList`'s default bottom-relative
    // scroll correction. An `if`/`else`, not `?? mutate()` — `preserveViewportPosition`
    // returns `void`, so `??` would read that as nullish and run `mutate` a second time,
    // silently cancelling the toggle it just performed.
    const preserveScrollFor = (anchor: HTMLElement, mutate: () => void) => {
        if (lazyListRef.current) lazyListRef.current.preserveViewportPosition(anchor, mutate)
        else mutate()
    }

    // Moves the context-usage bar during a run, not just once it ends: a run with tool calls makes several
    // model calls in sequence, each with its own context usage, and `turn_progress` is the per-call event that
    // carries the running context size.
    useServerEvent('turn_progress', (event) => {
        if (event.chat_id !== chatId || event.prompt_tokens == null) return
        setContextUsed(event.prompt_tokens + event.eval_tokens)
    })

    // Reacts to a run's end: a busy model server gets its popup, and a regenerate that didn't produce its reply gets the old
    // reply back (the notification for a finished run is the runs provider's, which works on every page)
    const regeneratingRef = useRef(false)
    runEndedRef.current = (end) => {
        if (end.reason === 'failed' && end.status === 423) setBusyReason(end.detail ?? 'The model is in use by someone else.')
        if (regeneratingRef.current) {
            regeneratingRef.current = false
            if (end.reason !== 'answered') void reload()
        }
    }

    const handleConfirm = async (decisions: Decision[]) => {
        const forChatId = chatId
        setTurnError(null)
        try {
            await answer(decisions, lastThinkRef.current)
        } catch (e) {
            turnFailed(forChatId, e, 'Something went wrong continuing that turn — try again.')
        }
    }

    // What the composer last asked for, reused for a turn the backend starts itself (a finished job)
    const lastThinkRef = useRef<ThinkChoice>(true)
    const [editing, setEditing] = useState<{ key: number; messageId: number; text: string; images: string[]; fileIds: number[] } | null>(null)
    const handleSend = async (prompt: string, think?: ThinkChoice, images?: string[], fileIds?: number[]) => {
        const forChatId = chatId
        lastThinkRef.current = think ?? true
        setTurnError(null)
        setSearchHighlight(null)
        // An edit sends the new text as an ordinary turn, once what it replaces is gone. If that
        // can't be removed the composer keeps the text, so nothing the user wrote is lost.
        if (editing) {
            try {
                await rewind(editing.messageId)
            } catch (e) {
                if (chatIdRef.current === forChatId) setTurnError(errorReason(e, "Couldn't replace that message — the chat is unchanged."))
                return
            }
            setEditing(null)
        }
        try {
            await send(prompt, think, images, fileIds)
        } catch (e) {
            turnFailed(forChatId, e, 'Something went wrong sending that — try again.')
        }
    }

    const handleRegenerate = async (messageId: number) => {
        const forChatId = chatId
        setTurnError(null)
        setSearchHighlight(null)
        // The old reply leaves the screen at once; the backend deletes it once the new one is stored, and it comes
        // back if the run ends without one
        regeneratingRef.current = true
        hideMessage(messageId)
        try {
            await regenerate(messageId, lastThinkRef.current)
        } catch (e) {
            regeneratingRef.current = false
            void reload()
            turnFailed(forChatId, e, "Couldn't regenerate that reply — the chat is unchanged.")
        }
    }

    const handleDelete = async (messageId: number) => {
        const forChatId = chatId
        setTurnError(null)
        setSearchHighlight(null)
        try {
            await rewind(messageId)
        } catch (e) {
            if (chatIdRef.current === forChatId) setTurnError(errorReason(e, "Couldn't delete that — the chat is unchanged."))
        }
    }

    const [confirming, setConfirming] = useState<{ kind: 'regenerate' | 'delete'; messageId: number; later: number } | null>(null)

    const sendRef = useRef(handleSend)
    sendRef.current = handleSend
    const [initialThink] = useState(() => peekPendingPrompt(chatId)?.think !== false)

    useEffect(() => {
        setExpandedTools({})
        setExpandedThinking({})
        setTurnError(null)
        setSearchHighlight(null)
        setEditing(null)
        setConfirming(null)
    }, [chatId])

    useEffect(() => {
        const pending = consumePendingPrompt(chatId)
        if (pending) sendRef.current(pending.prompt, pending.think, pending.images, pending.fileIds)
    }, [chatId])

    // A search hit may live in a page that isn't loaded yet: load older pages until it's
    // mounted, then scroll to it.
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

    // A chat opened from the all-chats search arrives with the message it was found by in the
    // navigation state. It waits for the chat's first page, then scrolls there like an in-chat
    // search hit, and clears the state so a refresh or Back doesn't jump again.
    const location = useLocation()
    const navigate = useNavigate()
    const jumpToMessageRef = useRef(jumpToMessage)
    jumpToMessageRef.current = jumpToMessage
    const jump = (location.state as { jump?: { hit: MessageSearchOut; query: string } } | null)?.jump
    useEffect(() => {
        if (!jump || !ready) return
        navigate({pathname: location.pathname, search: location.search}, {replace: true, state: null})
        void jumpToMessageRef.current(jump.hit, jump.query)
    }, [jump, ready, navigate, location.pathname, location.search])

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="chat">
                <ChatHeader chatId={chatId} name={chatName}
                            contextUsed={contextUsed}
                            contextMax={contextMax}
                            folderId={folderId}
                            onFolderChanged={setFolderId}
                            parentChatId={parentChatId ?? null}
                            onSelectSearchResult={jumpToMessage}
                            hasActiveHighlight={searchHighlight != null}
                            onClearHighlight={() => setSearchHighlight(null)}
                            toolsEnabled={chatToolsEnabled}
                            onToolsChanged={setChatToolsEnabled}
                            runActive={busy}/>
                <LazyList ref={lazyListRef} className="chat__list" threshold={LOAD_MORE_THRESHOLD}
                          onTopReached={loadOlder}>
                    {messages.map((m, i) => {
                        // Only a plain reply straight after a message of the user's can be regenerated
                        // (the backend checks the rest: tool calls, the compaction boundary)
                        // A message can be edited or deleted when nothing from it onward is anything but plain
                        // conversation (the backend checks the rest: the compaction boundary, plugin chats)
                        const plainFromHere = messages.slice(i).every((x) => x.id != null && (x.role === 'user' || x.role === 'assistant'))
                        const canCut = plainFromHere && !busy && parentChatId === null
                        const canRegenerate = i === messages.length - 1 && m.role === 'assistant' && m.id != null
                            && messages[i - 1]?.role === 'user' && !busy && parentChatId === null
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
                                            onRegenerate={canRegenerate && m.id != null ? () => setConfirming({kind: 'regenerate', messageId: m.id as number, later: 0}) : undefined}
                                            onEdit={canCut && m.role === 'user' && m.id != null
                                                ? () => setEditing({key: Date.now(), messageId: m.id as number, text: m.content, images: m.images ?? [], fileIds: m.file_ids ?? []})
                                                : undefined}
                                            onDelete={canCut && m.id != null
                                                ? () => setConfirming({kind: 'delete', messageId: m.id as number, later: messages.length - i - 1})
                                                : undefined}
                                            isLast={i === messages.length - 1}
                                        />
                                    )}
                                </div>
                            </Fragment>
                        )
                    })}
                    {busy ? <RunStatus view={turn} onStop={() => void stop()}/> : null}
                    {endLine != null ? <NoticeMessage content={endLine}/> : null}
                </LazyList>
                <ModelStateBanner watching={turn.status === 'running'}/>
                {turn.status === 'waiting_for_permission' ? (
                    <ToolConfirmation key={turn.lastEnd?.ended_at} pending={turn.pending} onConfirm={handleConfirm}/>
                ) : null}
                {turnError ? (
                    <Div className="chat__error">
                        <Label className="chat__error-text" text={turnError}/>
                        <Button variant="secondary" text="Dismiss" onClicked={() => setTurnError(null)}/>
                    </Div>
                ) : null}
                {parentChatId != null ? (
                    <Div className="chat__readonly">
                        <Label variant="secondary" text="This is a sub-agent's chat — it runs on its own, so there is nothing to send."/>
                    </Div>
                ) : (
                    <UserInput
                        blocked={busy}
                        onSended={handleSend}
                        inputDisabled={false}
                        clearOnSend={editing == null}
                        editing={editing}
                        onCancelEdit={() => setEditing(null)}
                        initialThink={initialThink}
                        chatId={chatId}
                        modelChoice={chatModel != null ? {provider: chatProvider, model: chatModel, profileId: chatProfileId} : null}
                        onModelPicked={(choice) => void changeModel(choice)}
                    />
                )}
            </Div>
            <ModelBusyPopup reason={busyReason} onClose={() => setBusyReason(null)}/>
            <ConfirmPopup
                open={confirming != null}
                title={confirming?.kind === 'regenerate' ? 'Regenerate reply' : 'Delete from here'}
                message={confirming?.kind === 'regenerate'
                    ? 'The model answers again, and the new reply replaces this one.'
                    : confirming?.later
                        ? `This message and the ${confirming.later} after it are deleted.`
                        : 'This message is deleted.'}
                confirmLabel={confirming?.kind === 'regenerate' ? 'Regenerate' : 'Delete'}
                onConfirm={() => {
                    if (!confirming) return
                    if (confirming.kind === 'regenerate') void handleRegenerate(confirming.messageId)
                    else void handleDelete(confirming.messageId)
                }}
                onClose={() => setConfirming(null)}
            />
        </Div>
    )
};

export default Chat
