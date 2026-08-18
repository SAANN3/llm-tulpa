import { useEffect, useRef, useState } from 'react'

import { allowScope } from '../api/agent/allow_scope'
import { canUseTool } from '../api/agent/can_use_tool'
import { chat as sendChatMessage } from '../api/agent/chat'
import { continueChat } from '../api/agent/continue_chat'
import type { AgentScopeGrant, AgentToolCall, ChatOut as AgentChatOut, UseToolOut } from '../api/agent/types'
import { useTool as runNextTool } from '../api/agent/use_tool'
import { getMessages } from '../api/chats/messages'
import type { MessageOut } from '../api/chats/types'
import { useSettings } from '../context/useSettings'
import { notify } from '../notifications'
import { peekPendingPrompt } from '../pendingPrompt'

const MESSAGES_PAGE_SIZE = 30

/**
 * A message as shown in the timeline — built either from a fetched `MessageOut` or
 * straight from an agent api response, never refetched afterwards. No `id`: messages
 * are only ever appended, so list position is a stable key. `thinking`/
 * `thought_duration_ms` are only ever set on an `assistant` message — user/tool turns
 * never produce a reasoning trace.
 */
export type DisplayMessage =
  | {
      role: 'user' | 'assistant'
      content: string
      created_at: string
      thinking?: string | null
      thought_duration_ms?: number | null
    }
  | { role: 'tool'; content: unknown; tool_name: string | null; created_at: string; arguments: Record<string, unknown> }

/**
 * Maps one fetched page of `MessageOut`s (already in chronological order) to
 * `DisplayMessage`s, pairing each `tool`-role message with the arguments it was called
 * with. Those arguments live only on the *assistant* message that requested the call
 * (`tool_calls[]`) — a `tool`-role message never carries its own — so this has to walk
 * the page in order rather than map each message in isolation: whenever an assistant
 * message with tool calls goes by, its calls are queued, and each `tool` message
 * that follows claims the next one off that queue (same position-based pairing the
 * backend itself relies on to resolve pending calls in order). A page boundary that
 * happens to split a run right between the assistant message and its results is the
 * one case this can't pair — rare, and just falls back to no arguments shown for those.
 */
function toDisplayMessages(page: MessageOut[]): DisplayMessage[] {
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
    return {
      role: m.role as 'user' | 'assistant',
      content: m.content,
      created_at: m.created_at,
      thinking: m.thinking,
      thought_duration_ms: m.thought_duration_ms,
    }
  })
}

function userMessage(content: string): DisplayMessage {
  return { role: 'user', content, created_at: new Date().toISOString() }
}

function assistantMessage(reply: AgentChatOut): DisplayMessage {
  return {
    role: 'assistant',
    content: reply.content,
    created_at: reply.created_at,
    thinking: reply.thinking,
    thought_duration_ms: reply.thought_duration_ms,
  }
}

function toolMessage(result: UseToolOut, args: Record<string, unknown>): DisplayMessage {
  return { role: 'tool', content: result.content, tool_name: result.tool_name, created_at: result.created_at, arguments: args }
}

/** Roughly how many characters of a single sentence read comfortably in a notification. */
const NOTIFICATION_BODY_MAX_CHARS = 100

/** Cuts `text` down to `NOTIFICATION_BODY_MAX_CHARS`, appending `...` if it was actually cut. */
function truncateForNotification(text: string): string {
  const trimmed = text.trim()
  if (trimmed.length <= NOTIFICATION_BODY_MAX_CHARS) return trimmed

  return `${trimmed.slice(0, NOTIFICATION_BODY_MAX_CHARS).trimEnd()}...`
}

/** What the caller decided about one pending tool call. */
export const ToolAllowance = {
  /** Don't run it. `useTool` gets no override, so the backend denies it (nothing's
   * granted) and records the decline — same outcome as not deciding at all. */
  Forbid: 'forbid',
  /** Run it this one time, via `useTool`'s one-time override. Never persisted. */
  OnlyNow: 'only_now',
  /** Persist the grant (`allowScope`) before running it, so future calls to this tool
   * in this chat won't need to ask again. */
  Permanent: 'permanent',
} as const
export type ToolAllowance = (typeof ToolAllowance)[keyof typeof ToolAllowance]

/**
 * One tool call from a reply that needs a decision before the turn can continue —
 * carries what `AgentToolCall.permission` said (`reason`/`escalation`) plus the call
 * itself, so a caller can show what's being asked for and why.
 */
export interface DangerousToolCall {
  name: string
  arguments: Record<string, unknown>
  reason: string
  escalation: AgentScopeGrant | null
}

/**
 * Tool calls needing a decision, keyed by their index in the triggering reply's
 * `tool_calls` array — not by name, since a single reply can call the same tool more
 * than once, and each of those calls needs its own independent decision.
 */
export type PendingConfirmations = Record<number, DangerousToolCall>

/** A decision per pending index — see `PendingConfirmations`. An index with no entry
 * here is treated the same as `ToolAllowance.Forbid`. */
export type Decisions = Record<number, ToolAllowance>

/**
 * What driving a turn produced: either it ran to completion (`needsConfirmation:
 * false`), or it hit one or more tool calls that aren't currently permitted and paused
 * (`needsConfirmation: true`) — `confirm` resumes it once the caller's decided, and
 * itself resolves to another `TurnResult`, since the model's next reply can turn out
 * to request more tool calls needing their own decisions.
 */
export type TurnResult =
  | { needsConfirmation: false; reply: AgentChatOut }
  | { needsConfirmation: true; pending: PendingConfirmations; confirm: (decisions: Decisions) => Promise<TurnResult> }

/*
 * How one turn actually flows through `driveTurn`/`resolveToolCallsAndContinue` below,
 * and where `send`'s `finishOrPause` (further down) plugs in as the pause/resume
 * boundary a caller sees. `driveTurn` and `resolveToolCallsAndContinue` call each other
 * back and forth — a reply with no pending confirmations runs straight through; one
 * that does pauses and hands back a `confirm` that re-enters the same loop once a
 * caller's decided what to do.
 *
 * send(prompt) --> chat API --> reply
 *                                 |
 *                                 v
 *                         driveTurn(reply)
 *                                 |
 *                 +---------------+---------------+
 *                 |                                |
 *           no tool calls                   has tool calls
 *                 |                                |
 *                 v                                v
 *              [DONE]              findPendingConfirmations
 *                                                   |
 *                             +---------------------+---------------------+
 *                             |                                           |
 *                       none denied                                 some denied
 *                             |                                           |
 *                             v                                           v
 *     resolveToolCallsAndContinue({})                       { needsConfirmation: true,
 *                             |                                pending, confirm }
 *                             |                                           |
 *                             |                              caller decides, calls
 *                             |                              confirm(decisions)
 *                             |                                           |
 *                             |                                           v
 *                             |                    resolveToolCallsAndContinue(decisions)
 *                             |                                           |
 *                             +--------------------+---------------------+
 *                                                   |
 *                                                   v
 *                1. allowScope() for each Permanent pick
 *                2. useTool() per pending call, in order
 *                   (one-time override if OnlyNow)
 *                3. continueChat() --> next reply
 *                                                   |
 *                                                   v
 *                             loops back to driveTurn(next reply)
 */

/** Picks out of `toolCalls` whichever ones `AgentToolCall.permission` denied — empty
 * when every requested call is already allowed (including "no calls at all"). */
function findPendingConfirmations(toolCalls: AgentToolCall[]): PendingConfirmations {
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
}

/**
 * Runs every remaining pending tool call for `chatId`, one `useTool` per call in the
 * order the backend reports them (same order `pending`'s indices refer to), then asks
 * the model to continue. `decisions`/`pending` decide what each call gets: `Permanent`
 * persists its grant via `allowScope` before the loop reaches it and then runs with no
 * override (the now-stored scope covers it); `OnlyNow` passes the offered scope as a
 * one-time override; anything else (`Forbid`, or no decision at all) passes nothing,
 * which the backend denies and records on its own — there's nothing this needs to do
 * differently for an explicit decline versus an unanswered one.
 */
async function resolveToolCallsAndContinue(
  chatId: number,
  think: boolean,
  decisions: Decisions,
  pending: PendingConfirmations,
  toolCalls: AgentToolCall[],
  onMessage: (message: DisplayMessage) => void,
): Promise<TurnResult> {
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
  onMessage(assistantMessage(reply))

  return driveTurn(chatId, think, reply, onMessage)
}

/**
 * Entry point for handling any reply that might carry tool calls, whether it's the
 * model's first response to a new prompt or a later one from `resolveToolCallsAndContinue`.
 * Delegates to `driveToolCalls` for everything past "does this reply even have tool
 * calls" — `resume` below enters at `driveToolCalls` directly, from `can_use_tool`
 * rather than a fresh reply, since a reconnect has no new reply to check first.
 */
async function driveTurn(
  chatId: number,
  think: boolean,
  reply: AgentChatOut,
  onMessage: (message: DisplayMessage) => void,
): Promise<TurnResult> {
  if (!reply.can_use_tools) {
    return { needsConfirmation: false, reply }
  }

  return driveToolCalls(chatId, think, reply.tool_calls, onMessage)
}

/**
 * The actual permission check + branch: run straight through if every call in
 * `toolCalls` is already permitted, otherwise pause and hand back a `confirm` that
 * resumes once the caller's decided what to do with whatever's pending. Shared by
 * `driveTurn` (a fresh reply's tool calls) and `resume` (whatever `can_use_tool`
 * reports is still outstanding for a chat, independent of any particular reply) —
 * from here on neither cares where `toolCalls` came from.
 */
async function driveToolCalls(
  chatId: number,
  think: boolean,
  toolCalls: AgentToolCall[],
  onMessage: (message: DisplayMessage) => void,
): Promise<TurnResult> {
  const pending = findPendingConfirmations(toolCalls)
  if (Object.keys(pending).length === 0) {
    return resolveToolCallsAndContinue(chatId, think, {}, pending, toolCalls, onMessage)
  }

  return {
    needsConfirmation: true,
    pending,
    confirm: (decisions) => resolveToolCallsAndContinue(chatId, think, decisions, pending, toolCalls, onMessage),
  }
}

/**
 * A chat's message timeline: the newest page loaded on mount/`chatId` change,
 * `loadOlder` to page further back, and `send` to run a full turn (optimistic user
 * message, then whatever the model/tools produce) — appending each message as it
 * arrives rather than refetching. `onAppended`, if given, fires after the initial load
 * and after every appended message (not after `loadOlder`, which prepends) — for a
 * caller that wants to snap a scroll view to the bottom on those and not the other.
 * `chatId` must already be a resolved chat — callers with no chat selected yet
 * shouldn't render whatever's using this hook at all.
 */
export function useMessages(chatId: number, onAppended?: () => void) {
  const { settings } = useSettings()
  const [messages, setMessages] = useState<DisplayMessage[]>([])
  const [total, setTotal] = useState(0)
  const [loadingMore, setLoadingMore] = useState(false)
  // Which chat's request is in flight, not just whether one is — this same hook
  // instance keeps running across a chat switch (see `chatIdRef` below), so a plain
  // boolean here would either get stuck `true` forever once its owning request stopped
  // being allowed to touch it (if guarded), or show the "thinking" animation on a chat
  // that has nothing in flight (if not). `sending`, below, is just whether it matches
  // whatever chat is currently on screen.
  const [sendingChatId, setSendingChatId] = useState<number | null>(null)
  const sending = sendingChatId === chatId
  // Whether this chat was left mid-turn (a tool call was pending when the tab closed or
  // navigated away) — fetched alongside the initial message page so a caller knows
  // right when the chat opens, without having to speculatively call `resume` itself.
  const [canContinue, setCanContinue] = useState(false)

  // Read through a ref rather than depending on `onAppended` directly — callers
  // typically pass a fresh arrow function each render, which would otherwise re-run
  // this effect (and refetch) on every unrelated re-render.
  const onAppendedRef = useRef(onAppended)
  onAppendedRef.current = onAppended

  // The chat this hook instance is *currently* showing, readable from inside a request
  // that was kicked off for a possibly-different `chatId` — this same hook instance
  // keeps running (React reuses it, only the `chatId` argument changes) when the caller
  // switches chats, so a request already in flight doesn't stop just because it's not
  // what's on screen anymore. `send`/`resume`/the initial load all check this before
  // applying their result, so a slow reply from chat A can't paint itself onto chat B.
  const chatIdRef = useRef(chatId)
  chatIdRef.current = chatId

  // Messages appended (via `send`/`appendMessage`) since the initial fetch for this
  // `chatId` started — e.g. a pending-prompt auto-send firing right after a brand new
  // chat is created and navigated to. The fetch can resolve *after* one of those
  // appends lands (it's a real network round-trip racing a synchronous local append),
  // so its `.then` merges historical + live-appended instead of blindly overwriting
  // `messages`, which would otherwise wipe out whatever was just optimistically added.
  const liveAppendedSinceFetchRef = useRef<DisplayMessage[]>([])

  // Reset during render, not inside the effect below — Strict Mode double-invokes
  // effects in dev (mount, cleanup, mount again) without re-rendering in between, and
  // the auto-send-pending-prompt effect (in Chat.tsx) isn't idempotent: its first
  // invocation already appends a live message (the user's just-sent prompt) before this
  // effect's second invocation would run. Resetting the ref there wiped that append out
  // from under it — the reported bug was the just-sent user message vanishing, leaving
  // only the assistant's reply once it came back. Comparing against a ref during render
  // is React's own documented pattern for "reset something when a prop changes," and is
  // safe under Strict Mode's double-render too: the first of the two (identical)
  // renders performs the reset and updates the ref, so the second sees no change and
  // skips it.
  const trackedChatIdRef = useRef<number | null>(null)
  // A pending prompt only ever exists for a chat that was *just* created (see
  // `setPendingPrompt`'s one call site, right after `createChat`) — so a chat that has
  // one is guaranteed to have zero prior messages. Captured once here (render-time,
  // same reasoning as the reset above) rather than checked inside the effect: the
  // initial `getMessages` fetch is a real network round-trip that can resolve *after*
  // the auto-send effect's `send()` has already persisted the user's message —
  // historical would then already include it, on top of it also being in
  // `liveAppendedSinceFetchRef`, showing it twice. Skipping the fetch outright for a
  // chat we already know starts empty removes the race instead of reconciling it.
  const skipInitialFetchRef = useRef(false)
  if (trackedChatIdRef.current !== chatId) {
    trackedChatIdRef.current = chatId
    liveAppendedSinceFetchRef.current = []
    skipInitialFetchRef.current = peekPendingPrompt(chatId) != null
  }

  useEffect(() => {
    // Guards against a slower fetch for a chat already navigated away from resolving
    // *after* a newer one and overwriting its (correct, current) result — e.g. clicking
    // through chats A -> B -> C fast enough that A's response lands last.
    let cancelled = false
    setCanContinue(false)

    if (!skipInitialFetchRef.current) {
      getMessages({ chatId, limit: MESSAGES_PAGE_SIZE }).then((result) => {
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
      const result = await getMessages({ chatId, skip, limit: MESSAGES_PAGE_SIZE })
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

  // Marks `requestChatId` as no longer having a request in flight — but only if it's
  // still the one `sendingChatId` points at. Without that check, a request for a chat
  // that's since been superseded by a *newer* request (for a different chat) finishing
  // later would clear the newer one's in-flight marker out from under it.
  const clearSending = (requestChatId: number) =>
    setSendingChatId((current) => (current === requestChatId ? null : current))

  // Fires the completion/pause notification and, for a paused turn, wraps `confirm` so
  // resuming it re-enters the same `sendingChatId`/notification handling —
  // `TurnResult`s can chain (a resumed turn's next reply can itself need confirmation),
  // so this has to apply itself again to whatever `confirm` eventually resolves to, not
  // just once. `requestChatId` is whatever chat this particular `send`/`resume` call
  // started for — checked against `chatIdRef` (the *current* chat) before firing a
  // notification, since `confirm` can be invoked long after the caller's navigated
  // elsewhere (unlike `sendingChatId`, notifications are keyed to what's on screen now,
  // not to a specific request).
  const finishOrPause = (requestChatId: number, result: TurnResult): TurnResult => {
    const isCurrent = () => chatIdRef.current === requestChatId

    if (!result.needsConfirmation) {
      // Fires once per turn, after the final assistant reply — not per intermediate
      // tool-call message, those are noise for this purpose.
      if (isCurrent() && settings?.notifications_enabled) {
        notify('llm-tulpa', truncateForNotification(result.reply.content))
      }
      return result
    }

    if (isCurrent() && settings?.notifications_enabled) {
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

  // Looking for how a turn actually flows (the pause-for-confirmation loop)? See the
  // diagram above `findPendingConfirmations` earlier in this file.
  const send = async (prompt: string, think = true): Promise<TurnResult> => {
    const requestChatId = chatId
    const guardedAppend = (message: DisplayMessage) => {
      if (chatIdRef.current === requestChatId) appendMessage(message)
    }

    guardedAppend(userMessage(prompt))
    setSendingChatId(requestChatId)
    try {
      const reply = await sendChatMessage(chatId, prompt, think)
      guardedAppend(assistantMessage(reply))
      return finishOrPause(requestChatId, await driveTurn(chatId, think, reply, guardedAppend))
    } finally {
      clearSending(requestChatId)
    }
  }

  // For picking a chat back up with no new prompt to send — e.g. the tab was closed or
  // switched away from mid-turn, so whatever `confirm` `send` last handed out is long
  // gone along with the rest of that JS session. Asks the backend fresh (`can_use_tool`
  // reflects the database, not anything held in memory here) whether this chat still
  // has unresolved tool calls; `null` if not, meaning there's nothing to resume and the
  // already-loaded message history is simply where things stand. Otherwise runs through
  // the exact same `driveToolCalls` branch a live turn would, so a still-pending
  // permission pauses here as if the browser had never gone away.
  const resume = async (think = true): Promise<TurnResult | null> => {
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

  return { messages, loadOlder, send, resume, sending, canContinue }
}
