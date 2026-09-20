import {useEffect, useRef, useState} from 'react'
import {allowScope} from '../api/agent/allow-scope.ts'
import {canUseTool} from '../api/agent/can-use-tool.ts'
import {chat as sendChatMessage} from '../api/agent/chat'
import {continueChat} from '../api/agent/continue-chat.ts'
import {jobNotices} from '../api/agent/job-notices.ts'
import type {
    AgentScopeGrant,
    AgentToolCall,
    ChatOut as AgentChatOut,
    NoticeOut,
    ThinkChoice,
    UseToolOut
} from '../api/agent/types'
import {useTool as runNextTool} from '../api/agent/use-tool.ts'
import {getMessages} from '../api/chats/messages'
import type {MessageOut} from '../api/chats/types'
import {useSettings} from '../context/use-settings.ts'
import {getAutoConfirm} from '../utils/auto-confirm.ts'
import {notify} from '../utils/notifications'
import {peekPendingPrompt} from '../utils/pending-prompt.ts'

const MESSAGES_PAGE_SIZE = 30

export type DisplayMessage =
    | {
    role: 'user' | 'assistant'
    content: string
    created_at: string
    thinking?: string | null
    thought_duration_ms?: number | null
    images?: string[]
    file_ids?: number[]
}
    | {
    role: 'tool';
    content: unknown;
    tool_name: string | null;
    created_at: string;
    arguments: Record<string, unknown>
}
    | { role: 'notice'; content: string; created_at: string }

/** Maps a fetched page of messages to display form, pairing each tool message with its call arguments */
const toDisplayMessages = (page: MessageOut[]): DisplayMessage[] => {
    let queuedArgs: Record<string, unknown>[] = []

    return page.map((m) => {
        if (m.role === 'assistant' && m.tool_calls.length > 0) {
            queuedArgs = m.tool_calls.map((call) => call.arguments)
        }
        if (m.role === 'tool') {
            return {
                role: 'tool',
                content: m.content,
                tool_name: m.tool_name,
                created_at: m.created_at,
                arguments: queuedArgs.shift() ?? {},
            }
        }
        if (m.role === 'notice') {
            return {role: 'notice', content: m.content, created_at: m.created_at}
        }
        return {
            role: m.role as 'user' | 'assistant',
            content: m.content,
            created_at: m.created_at,
            thinking: m.thinking,
            thought_duration_ms: m.thought_duration_ms,
            images: m.images,
            file_ids: m.file_ids,
        }
    })
};

const userMessage = (content: string, images: string[], fileIds: number[]): DisplayMessage => ({
    role: 'user',
    content,
    created_at: new Date().toISOString(),
    images,
    file_ids: fileIds
});

const assistantMessage = (reply: AgentChatOut): DisplayMessage => ({
    role: 'assistant',
    content: reply.content,
    created_at: reply.created_at,
    thinking: reply.thinking,
    thought_duration_ms: reply.thought_duration_ms,
    file_ids: reply.file_ids,
});

const noticeMessage = (notice: NoticeOut): DisplayMessage => ({
    role: 'notice',
    content: notice.content,
    created_at: notice.created_at,
});

/** Appends a reply as it was stored: the job notices persisted just before it, then the reply itself */
const appendReply = (reply: AgentChatOut, onMessage: (message: DisplayMessage) => void) => {
    reply.notices.forEach((notice) => onMessage(noticeMessage(notice)))
    onMessage(assistantMessage(reply))
};

const toolMessage = (result: UseToolOut, args: Record<string, unknown>): DisplayMessage => ({
    role: 'tool',
    content: result.content,
    tool_name: result.tool_name,
    created_at: result.created_at,
    arguments: args
});

const NOTIFICATION_BODY_MAX_CHARS = 100

const truncateForNotification = (text: string): string => {
    const trimmed = text.trim()
    if (trimmed.length <= NOTIFICATION_BODY_MAX_CHARS) return trimmed

    return `${trimmed.slice(0, NOTIFICATION_BODY_MAX_CHARS).trimEnd()}...`
};

export const ToolAllowance = {
    Forbid: 'forbid',
    OnlyNow: 'only_now',
    Permanent: 'permanent',
} as const
export type ToolAllowance = (typeof ToolAllowance)[keyof typeof ToolAllowance]

export interface DangerousToolCall {
    name: string
    arguments: Record<string, unknown>
    reason: string
    escalation: AgentScopeGrant | null
}

export type PendingConfirmations = Record<number, DangerousToolCall>

export type Decisions = Record<number, ToolAllowance>

export type TurnResult =
    | { needsConfirmation: false; reply: AgentChatOut }
    | { needsConfirmation: true; pending: PendingConfirmations; confirm: (decisions: Decisions) => Promise<TurnResult> }


/** Picks out whichever tool calls were denied and still need a decision */
const findPendingConfirmations = (toolCalls: AgentToolCall[]): PendingConfirmations => {
    const pending: PendingConfirmations = {}

    toolCalls.forEach((call, index) => {
        if (call.permission.status === 'denied') {
            pending[index] = {
                name: call.name,
                arguments: call.arguments,
                reason: call.permission.reason,
                escalation: call.permission.escalation,
            }
        }
    })

    return pending
};

/** Runs every remaining pending tool call for a chat, then asks the model to continue */
const resolveToolCallsAndContinue = async (
    chatId: number,
    think: ThinkChoice,
    decisions: Decisions,
    pending: PendingConfirmations,
    toolCalls: AgentToolCall[],
    onMessage: (message: DisplayMessage) => void,
): Promise<TurnResult> => {
    for (const [indexStr, allowance] of Object.entries(decisions)) {
        if (allowance !== ToolAllowance.Permanent) continue
        const call = pending[Number(indexStr)]
        if (call?.escalation) await allowScope(chatId, call.name, call.escalation.scope)
    }

    let index = 0
    let toolsLeft = true
    while (toolsLeft) {
        const overrideScope = decisions[index] === ToolAllowance.OnlyNow ? pending[index]?.escalation?.scope : undefined

        const result = await runNextTool(chatId, overrideScope)
        onMessage(toolMessage(result, toolCalls[index]?.arguments ?? {}))
        toolsLeft = result.tools.length > 0
        index += 1
    }

    const reply = await continueChat(chatId, think)
    appendReply(reply, onMessage)

    return driveTurn(chatId, think, reply, onMessage)
};

/** Entry point for handling a reply that might carry tool calls */
const driveTurn = async (
    chatId: number,
    think: ThinkChoice,
    reply: AgentChatOut,
    onMessage: (message: DisplayMessage) => void,
): Promise<TurnResult> => {
    if (!reply.can_use_tools) {
        return {needsConfirmation: false, reply}
    }

    return driveToolCalls(chatId, think, reply.tool_calls, onMessage)
};

/** Runs tool calls straight through if already permitted, otherwise pauses for confirmation */
const driveToolCalls = async (
    chatId: number,
    think: ThinkChoice,
    toolCalls: AgentToolCall[],
    onMessage: (message: DisplayMessage) => void,
): Promise<TurnResult> => {
    const pending = findPendingConfirmations(toolCalls)
    if (Object.keys(pending).length === 0) {
        return resolveToolCallsAndContinue(chatId, think, {}, pending, toolCalls, onMessage)
    }

    return {
        needsConfirmation: true,
        pending,
        confirm: (decisions) => resolveToolCallsAndContinue(chatId, think, decisions, pending, toolCalls, onMessage),
    }
};

/** A chat's message timeline, with loadOlder to page back and send to run a full turn */
export const useMessages = (chatId: number, onAppended?: () => void) => {
    const {settings} = useSettings()
    const [messages, setMessages] = useState<DisplayMessage[]>([])
    const [total, setTotal] = useState(0)
    const [loadingMore, setLoadingMore] = useState(false)
    const [sendingChatId, setSendingChatId] = useState<number | null>(null)
    const sending = sendingChatId === chatId
    const [canContinue, setCanContinue] = useState(false)
    const onAppendedRef = useRef(onAppended)
    onAppendedRef.current = onAppended

    const chatIdRef = useRef(chatId)
    chatIdRef.current = chatId
    const liveAppendedSinceFetchRef = useRef<DisplayMessage[]>([])
    const trackedChatIdRef = useRef<number | null>(null)
    const skipInitialFetchRef = useRef(false)
    if (trackedChatIdRef.current !== chatId) {
        trackedChatIdRef.current = chatId
        liveAppendedSinceFetchRef.current = []
        skipInitialFetchRef.current = peekPendingPrompt(chatId) != null
    }

    useEffect(() => {
        let cancelled = false
        setCanContinue(false)

        if (!skipInitialFetchRef.current) {
            getMessages({chatId, limit: MESSAGES_PAGE_SIZE}).then((result) => {
                if (cancelled) return
                const historical = toDisplayMessages([...result.messages].reverse())
                setMessages([...historical, ...liveAppendedSinceFetchRef.current])
                setTotal(result.total + liveAppendedSinceFetchRef.current.length)
                onAppendedRef.current?.()
            })
        }

        canUseTool(chatId).then((status) => {
            if (!cancelled) setCanContinue(status.can_use)
        })

        return () => {
            cancelled = true
        }
    }, [chatId])

    const loadOlder = async () => {
        if (loadingMore || messages.length >= total) return

        setLoadingMore(true)
        try {
            const skip = messages.length
            const result = await getMessages({chatId, skip, limit: MESSAGES_PAGE_SIZE})
            const older = toDisplayMessages([...result.messages].reverse())
            setMessages((prev) => [...older, ...prev])
            setTotal(result.total)
        } finally {
            setLoadingMore(false)
        }
    }

    const appendMessage = (message: DisplayMessage) => {
        liveAppendedSinceFetchRef.current = [...liveAppendedSinceFetchRef.current, message]
        setMessages((prev) => [...prev, message])
        setTotal((t) => t + 1)
        onAppendedRef.current?.()
    }

    const clearSending = (requestChatId: number) =>
        setSendingChatId((current) => (current === requestChatId ? null : current))

    const finishOrPause = (requestChatId: number, result: TurnResult): TurnResult => {
        const isCurrent = () => chatIdRef.current === requestChatId

        if (!result.needsConfirmation) {

            if (isCurrent() && settings?.notifications_enabled && result.reply.content.trim().length > 0) {
                notify('llm-tulpa', truncateForNotification(result.reply.content))
            }
            return result
        }

        if (isCurrent() && settings?.notifications_enabled && !getAutoConfirm()) {
            const names = Object.values(result.pending)
                .map((call) => call.name)
                .join(', ')
            notify('llm-tulpa', `Waiting on your OK to run: ${names}`)
        }

        return {
            ...result,
            confirm: async (decisions) => {
                setSendingChatId(requestChatId)
                try {
                    return finishOrPause(requestChatId, await result.confirm(decisions))
                } finally {
                    clearSending(requestChatId)
                }
            },
        }
    }

    /** Sends a prompt as a new turn, from an optimistic user message through any tool calls */
    const send = async (prompt: string, think: ThinkChoice = true, images: string[] = [], fileIds: number[] = []): Promise<TurnResult> => {
        const requestChatId = chatId
        const guardedAppend = (message: DisplayMessage) => {
            if (chatIdRef.current === requestChatId) appendMessage(message)
        }

        guardedAppend(userMessage(prompt, images, fileIds))
        setSendingChatId(requestChatId)
        try {
            const reply = await sendChatMessage(chatId, prompt, think, images, fileIds)
            appendReply(reply, guardedAppend)
            return finishOrPause(requestChatId, await driveTurn(chatId, think, reply, guardedAppend))
        } finally {
            clearSending(requestChatId)
        }
    }

    /** Picks a chat back up when it has an unresolved tool call left from a prior session */
    const resume = async (think: ThinkChoice = true): Promise<TurnResult | null> => {
        const requestChatId = chatId
        const guardedAppend = (message: DisplayMessage) => {
            if (chatIdRef.current === requestChatId) appendMessage(message)
        }

        const status = await canUseTool(chatId)
        if (!status.can_use) return null

        setSendingChatId(requestChatId)
        try {
            return finishOrPause(requestChatId, await driveToolCalls(chatId, think, status.tools, guardedAppend))
        } finally {
            clearSending(requestChatId)
        }
    }

    /** Asks the backend to report finished background jobs to the model and drives its reply like any turn; null when there was nothing to report */
    const runJobNotices = async (think: ThinkChoice = true): Promise<TurnResult | null> => {
        const requestChatId = chatId
        const guardedAppend = (message: DisplayMessage) => {
            if (chatIdRef.current === requestChatId) appendMessage(message)
        }

        setSendingChatId(requestChatId)
        try {
            const reply = await jobNotices(chatId, think)
            if (!reply) return null

            appendReply(reply, guardedAppend)
            return finishOrPause(requestChatId, await driveTurn(chatId, think, reply, guardedAppend))
        } finally {
            clearSending(requestChatId)
        }
    }

    return {messages, loadOlder, send, resume, runJobNotices, sending, canContinue}
};
