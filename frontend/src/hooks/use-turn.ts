import {useEffect, useRef, useState} from 'react'
import {answer as sendAnswer} from '../api/agent/answer.ts'
import {regenerateChat} from '../api/agent/regenerate-chat.ts'
import {startTurn} from '../api/agent/start-turn.ts'
import {stopTurn} from '../api/agent/stop.ts'
import type {AgentToolCall, Decision, ReplySoFar, RunEnded, StartTurnOut, ThinkChoice, TurnState, TurnStatus} from '../api/agent/types.ts'
import {getTurnState} from '../api/agent/turn-state.ts'
import {useServerEvent} from './use-server-events.ts'

/** What the page knows about a chat's turn: times are epoch milliseconds */
export interface TurnView {
    status: TurnStatus
    startedAt: number | null
    /** When the model call in flight started; null while a tool runs */
    callStartedAt: number | null
    evalTokens: number
    step: number
    stepLimit: number | null
    runningTool: string | null
    toolStartedAt: number | null
    /** While waiting for permission: the pending tool calls, in order */
    pending: AgentToolCall[]
    lastEnd: RunEnded | null
    /** The reply being written when the state was read (events don't change it: `useLiveReply` follows those) */
    reply: ReplySoFar | null
}

const IDLE: TurnView = {
    status: 'idle',
    startedAt: null,
    callStartedAt: null,
    evalTokens: 0,
    step: 0,
    stepLimit: null,
    runningTool: null,
    toolStartedAt: null,
    pending: [],
    lastEnd: null,
    reply: null,
}

const toMillis = (iso: string | null): number | null => (iso == null ? null : Date.parse(iso))

const fromState = (state: TurnState): TurnView => ({
    status: state.status,
    startedAt: toMillis(state.started_at),
    callStartedAt: toMillis(state.call_started_at),
    evalTokens: state.eval_tokens,
    step: state.step,
    stepLimit: state.step_limit,
    runningTool: state.running_tool,
    toolStartedAt: toMillis(state.tool_started_at),
    pending: state.pending,
    lastEnd: state.last_end,
    reply: state.reply,
})

export interface TurnHandlers {
    /** Messages were stored in the chat: read the ones newer than the last one shown */
    onMessagesChanged: () => void
    /** The run ended; `view` is the state it left the chat in (the pending calls, for a wait for permission) */
    onRunEnded: (end: RunEnded, view: TurnView) => void
}

/**
 * Follows a chat's turn, which the backend runs: its state is read once when the chat opens (so a reloaded
 * page shows "thinking for 1m 30s" from the real start) and kept current by the server events. The page
 * only starts a run, answers a permission prompt or stops one; a closed tab stops nothing.
 */
export const useTurn = (chatId: number, handlers: TurnHandlers) => {
    const [view, setView] = useState<TurnView>(IDLE)
    const handlersRef = useRef(handlers)
    handlersRef.current = handlers
    const chatIdRef = useRef(chatId)
    chatIdRef.current = chatId

    // Bumped by every event: a state read that was started before an event and answered after it is out of date,
    // and must not put back what the event just replaced (a run that failed quickly shown as running again)
    const eventSeqRef = useRef(0)
    const sawEvent = () => {
        eventSeqRef.current += 1
    }

    /** Reads the state from the backend: on opening the chat, after a wait for permission, when the tab comes back and when the event stream (re)opens. `force` applies it even if an event came meanwhile */
    const refresh = async (force = false) => {
        const forChatId = chatId
        const seq = eventSeqRef.current
        try {
            const state = await getTurnState(forChatId)
            if (chatIdRef.current === forChatId && (force || eventSeqRef.current === seq)) setView(fromState(state))
            return fromState(state)
        } catch {
            // The next event or refresh tries again
            return null
        }
    }
    const refreshRef = useRef(refresh)
    refreshRef.current = refresh

    useEffect(() => {
        setView(IDLE)
        void refreshRef.current()
    }, [chatId])

    // A background tab's connection can miss events: what happened meanwhile is read once it is shown again
    useEffect(() => {
        const onVisible = () => {
            if (document.visibilityState !== 'visible') return
            void refreshRef.current()
            handlersRef.current.onMessagesChanged()
        }
        document.addEventListener('visibilitychange', onVisible)
        return () => document.removeEventListener('visibilitychange', onVisible)
    }, [])

    // The stream (re)opened: events sent while it was down are gone, so the state and the messages are read again
    useServerEvent('stream_open', () => {
        void refreshRef.current()
        handlersRef.current.onMessagesChanged()
    })

    useServerEvent('run_started', (event) => {
        if (event.chat_id !== chatId) return
        sawEvent()
        setView({...IDLE, status: 'running', startedAt: Date.parse(event.started_at), callStartedAt: Date.now()})
        // The step limit is only in the state: taken from it for a run that is still going, never replacing what
        // events have said since
        void getTurnState(chatId).then((state) => {
            if (chatIdRef.current !== chatId) return
            setView((prev) => (prev.status === 'running' ? {...prev, stepLimit: state.step_limit} : prev))
        }, () => undefined)
    })

    useServerEvent('turn_progress', (event) => {
        if (event.chat_id !== chatId) return
        sawEvent()
        // Events carry what one model call generated; the state read on opening has the total so far
        setView((prev) => ({...prev, evalTokens: prev.evalTokens + event.eval_tokens, step: Math.max(prev.step, event.step)}))
    })

    useServerEvent('tool_started', (event) => {
        if (event.chat_id !== chatId) return
        sawEvent()
        setView((prev) => ({...prev, runningTool: event.tool_name, toolStartedAt: Date.now(), callStartedAt: null}))
    })

    useServerEvent('messages_changed', (event) => {
        if (event.chat_id !== chatId) return
        sawEvent()
        // A tool's result was stored: the model is asked again, and its call starts about now
        setView((prev) => (prev.runningTool != null
            ? {...prev, runningTool: null, toolStartedAt: null, callStartedAt: Date.now()}
            : prev))
        handlersRef.current.onMessagesChanged()
    })

    useServerEvent('run_ended', (event) => {
        if (event.chat_id !== chatId) return
        sawEvent()
        const end: RunEnded = {
            reason: event.reason,
            detail: event.detail,
            ended_at: new Date().toISOString(),
            started_at: event.started_at,
            eval_tokens: event.eval_tokens,
            status: event.status,
        }
        handlersRef.current.onMessagesChanged()
        if (event.reason === 'waiting_for_permission') {
            // The calls waiting for an answer are read from the backend
            void refreshRef.current(true).then((next) => handlersRef.current.onRunEnded(end, next ?? {...IDLE, lastEnd: end}))
            return
        }
        const next: TurnView = {...IDLE, lastEnd: end}
        setView(next)
        handlersRef.current.onRunEnded(end, next)
    })

    /** Marks the chat as running until `run_started` gives the real start; a failed request puts back what the backend says */
    const optimisticRun = () => setView((prev) => ({
        ...IDLE,
        status: 'running',
        startedAt: Date.now(),
        callStartedAt: Date.now(),
        stepLimit: prev.stepLimit,
    }))

    const guarded = async <T>(action: () => Promise<T>): Promise<T> => {
        optimisticRun()
        try {
            return await action()
        } catch (error) {
            void refreshRef.current()
            throw error
        }
    }

    const send = async (prompt: string, think: ThinkChoice = true, images: string[] = [], fileIds: number[] = []): Promise<StartTurnOut> => {
        const started = await guarded(() => startTurn(chatId, prompt, think, images, fileIds))
        handlersRef.current.onMessagesChanged()
        return started
    }

    const regenerate = (messageId: number, think: ThinkChoice = true) => guarded(() => regenerateChat(chatId, messageId, think))

    const answer = (decisions: Decision[], think: ThinkChoice = true) => guarded(() => sendAnswer(chatId, decisions, think))

    const stop = async () => {
        try {
            await stopTurn(chatId)
        } catch {
            // Nothing to stop any more: the run ended first
            void refreshRef.current()
        }
    }

    return {view, send, regenerate, answer, stop, refresh}
}
