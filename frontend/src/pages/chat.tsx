import {Fragment, useEffect, useRef, useState} from 'react'
import {Navigate, useSearchParams} from 'react-router-dom'
import '../styles/chat.scss'
import type {ThinkChoice} from '../api/agent/types'
import {getChats} from '../api/chats/get'
import {ChatMessage} from '../components/chat-message.tsx'
import {DateSeparator} from '../components/date-separator.tsx'
import type {LazyListHandle} from '../components/lazy-list.tsx'
import {LazyList} from '../components/lazy-list.tsx'
import {PendingAssistantMessage} from '../components/pending-assistant-message.tsx'
import {Button, Div, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {ToolConfirmation} from '../components/tool-confirmation.tsx'
import {ToolMessage} from '../components/tool-message.tsx'
import {UserInput} from '../components/user-input.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import type {Decisions, PendingConfirmations, TurnResult} from '../hooks/use-messages.ts'
import {ToolAllowance, useMessages} from '../hooks/use-messages.ts'
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
    const {messages, loadOlder, send, resume, sending, canContinue} = useMessages(chatId, () =>
        lazyListRef.current?.jumpToBottom(),
    )

    const [chatName, setChatName] = useState<string | null>(null)
    useDocumentTitle(chatName ?? 'Chat')

    useEffect(() => {
        setChatName(null)
        let cancelled = false

        getChats({id: chatId}).then((result) => {
            if (cancelled) return
            if (!('chats' in result)) setChatName(result.name)
        })

        return () => {
            cancelled = true
        }
    }, [chatId])

    const [expandedTools, setExpandedTools] = useState<Record<number, boolean>>({})
    const toggleToolExpanded = (index: number) =>
        setExpandedTools((prev) => ({...prev, [index]: !prev[index]}))
    const [pausedTurn, setPausedTurn] = useState<PausedTurn | null>(null)
    const [turnError, setTurnError] = useState<string | null>(null)
    const chatIdRef = useRef(chatId)
    chatIdRef.current = chatId

    const handleTurnResult = (forChatId: number, result: TurnResult) => {
        if (chatIdRef.current !== forChatId) return
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
    const handleSend = async (prompt: string, think?: ThinkChoice, images?: string[], fileIds?: number[]) => {
        const forChatId = chatId
        setTurnError(null)
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
    const [initialThink] = useState(() => peekPendingPrompt(chatId)?.think !== false)

    useEffect(() => {
        setPausedTurn(null)
        setExpandedTools({})
        setTurnError(null)
    }, [chatId])

    useEffect(() => {
        const pending = consumePendingPrompt(chatId)
        if (pending) sendRef.current(pending.prompt, pending.think, pending.images, pending.fileIds)
    }, [chatId])

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

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="chat">
                <LazyList ref={lazyListRef} className="chat__list" threshold={LOAD_MORE_THRESHOLD}
                          onTopReached={loadOlder}>
                    {messages.map((m, i) => {
                        const prev = messages[i - 1]
                        const showDate = !prev || !isSameDay(new Date(m.created_at), new Date(prev.created_at))
                        return (
                            <Fragment key={i}>
                                {showDate ? <DateSeparator date={new Date(m.created_at)}/> : null}
                                {m.role === 'tool' ? (
                                    <ToolMessage
                                        tool_name={m.tool_name ?? m.role}
                                        content={m.content}
                                        created_at={m.created_at}
                                        arguments={m.arguments}
                                        expanded={expandedTools[i] ?? false}
                                        onToggle={() => toggleToolExpanded(i)}
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
                                    />
                                )}
                            </Fragment>
                        )
                    })}
                    {sending ? <PendingAssistantMessage/> : null}
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
                />
            </Div>
        </Div>
    )
};

export default Chat
