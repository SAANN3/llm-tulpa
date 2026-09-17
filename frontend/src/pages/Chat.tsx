import { Fragment, useEffect, useRef, useState } from 'react'
import { Navigate, useSearchParams } from 'react-router-dom'

import '../styles/Chat.scss'
import type { ThinkChoice } from '../api/agent/types'
import { getChats } from '../api/chats/get'
import { ChatMessage } from '../components/ChatMessage'
import { DateSeparator } from '../components/DateSeparator'
import type { LazyListHandle } from '../components/LazyList'
import { LazyList } from '../components/LazyList'
import { PendingAssistantMessage } from '../components/PendingAssistantMessage'
import { Button, Div, Label } from '../components/primitives'
import { Sidebar } from '../components/Sidebar'
import { ToolConfirmation } from '../components/ToolConfirmation'
import { ToolMessage } from '../components/ToolMessage'
import { UserInput } from '../components/UserInput'
import { useDocumentTitle } from '../hooks/useDocumentTitle'
import type { Decisions, PendingConfirmations, TurnResult } from '../hooks/useMessages'
import { ToolAllowance, useMessages } from '../hooks/useMessages'
import { getAutoConfirm } from '../utils/autoConfirm'
import { consumePendingPrompt, peekPendingPrompt } from '../utils/pendingPrompt'
import { isSameDay } from '../utils/dates'

/** Auto-confirm mode's own stand-in for a human's decision on every pending call: grant
 * the escalation permanently when one's offered (matches clicking "Always in this
 * chat" — the point of the mode is to stop asking, not to re-prompt for the same tool
 * every single time), or acknowledge the decline when there's nothing to grant at all
 * (matches clicking "OK" on an unapprovable call — see `ToolConfirmation`). */
function autoConfirmDecisions(pending: PendingConfirmations): Decisions {
  const decisions: Decisions = {}
  for (const key of Object.keys(pending)) {
    const index = Number(key)
    decisions[index] = pending[index].escalation ? ToolAllowance.Permanent : ToolAllowance.Forbid
  }
  return decisions
}

/** A paused turn's confirm callback, held onto until the user decides. */
interface PausedTurn {
  pending: PendingConfirmations
  confirm: (decisions: Decisions) => Promise<TurnResult>
}

/** Trigger the next older page once the scroll position gets this close to the top, in px. */
const LOAD_MORE_THRESHOLD = 80

/** Route guard for `/chat` — `id` drives which chat is open, so without a valid one there's nothing to render. */
function Chat() {
  const [searchParams] = useSearchParams()
  const idParam = searchParams.get('id')
  const chatId = idParam == null ? NaN : Number(idParam)

  if (idParam == null || !Number.isInteger(chatId)) return <Navigate to="/" replace />

  return <ChatView chatId={chatId} />
}

function ChatView({ chatId }: { chatId: number }) {
  const lazyListRef = useRef<LazyListHandle>(null)
  const { messages, loadOlder, send, resume, sending, canContinue } = useMessages(chatId, () =>
    lazyListRef.current?.jumpToBottom(),
  )

  // Falls back to the 'Chat' placeholder until the name's actually fetched (or if the
  // fetch fails/the chat's gone) — never left blank.
  const [chatName, setChatName] = useState<string | null>(null)
  useDocumentTitle(chatName ?? 'Chat')

  useEffect(() => {
    setChatName(null)
    let cancelled = false

    getChats({ id: chatId }).then((result) => {
      if (cancelled) return
      if (!('chats' in result)) setChatName(result.name)
    })

    return () => {
      cancelled = true
    }
  }, [chatId])

  // Per-message-index override of the collapsed default for `ToolMessage`. `ChatView` is
  // reused across a chat switch (see the comment on `chatIdRef` below), so this needs its
  // own reset alongside `pausedTurn`'s — otherwise a tool card left expanded in one chat
  // would still show expanded after switching to another.
  const [expandedTools, setExpandedTools] = useState<Record<number, boolean>>({})
  const toggleToolExpanded = (index: number) =>
    setExpandedTools((prev) => ({ ...prev, [index]: !prev[index] }))

  // Set whenever a turn pauses on a call needing a decision — cleared once `confirm`
  // resolves to a `TurnResult` that doesn't need one (possibly after several rounds, if
  // the model's next reply asks for more tools).
  const [pausedTurn, setPausedTurn] = useState<PausedTurn | null>(null)

  // Set whenever `send`/`confirm`/`resume` rejects — a turn can fail for reasons that
  // have nothing to do with anything the user did (Ollama itself erroring or crashing
  // mid-reply, a network hiccup, ...), and none of those get persisted as a message or
  // recorded anywhere the backend can report back on later. Without this, that failure
  // was previously an unhandled promise rejection: the "Thinking" indicator would just
  // quietly disappear with zero explanation, indistinguishable from the tab having sat
  // in the background so long its own elapsed-time display looked stale — leaving a
  // real, silent backend/model failure looking like nothing happened at all.
  const [turnError, setTurnError] = useState<string | null>(null)

  // `ChatView` is reused across a chat switch (only `chatId` changes), so a turn kicked
  // off for the old chat can still be running when its result comes back — checked
  // against this before touching `pausedTurn`, so a slow reply from a chat that's no
  // longer open can't pop the confirmation panel over whatever's open now.
  const chatIdRef = useRef(chatId)
  chatIdRef.current = chatId

  const handleTurnResult = (forChatId: number, result: TurnResult) => {
    if (chatIdRef.current !== forChatId) return

    // Read fresh rather than kept in state — this page never needs to re-render on
    // the setting's own value, only to consult it at the moment a turn actually
    // pauses, and re-reading avoids this needing any cross-tab/cross-page sync with
    // wherever Settings last changed it.
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

    setPausedTurn(result.needsConfirmation ? { pending: result.pending, confirm: result.confirm } : null)
  }
  const handleConfirm = async (decisions: Decisions) => {
    if (!pausedTurn) return
    // Cleared right away rather than after `confirm` resolves — the decision's made,
    // and the pending calls it unblocks post their own messages as they run (see
    // `resolveToolCallsAndContinue`), so there's nothing left for this panel to show
    // while the turn plays out. It'll reappear via `handleTurnResult` below if the
    // model's next reply itself needs a fresh decision.
    const { confirm } = pausedTurn
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

  // Read through refs rather than depending on `send`/`resume` directly — both are
  // fresh closures every render (see `useMessages`), which would otherwise re-run this
  // on every unrelated re-render instead of once per `chatId`.
  const sendRef = useRef(handleSend)
  sendRef.current = handleSend
  const resumeRef = useRef(resume)
  resumeRef.current = resume

  // Seeded synchronously (not in the effect below) so the composer's "Thinking" toggle
  // already shows the right value on first render — the effect that actually consumes
  // and sends the pending prompt runs after mount, which would otherwise show a
  // flash of the default before snapping to the real value.
  // `peekPendingPrompt`'s `think` may now be a specific effort-level string (a mode
  // chosen on Home's composer before this chat existed) — only the on/off toggle
  // state matters for this particular seed (see `UserInput.initialThink`'s own doc
  // comment), so anything other than a literal `false` means "thinking was on."
  const [initialThink] = useState(() => peekPendingPrompt(chatId)?.think !== false)

  // Switching chats reuses this same `ChatView` instance (just a new `chatId` prop), so
  // a pause left over from the chat just navigated away from would otherwise still be
  // showing here — clear it immediately; the `canContinue` effect below repopulates it
  // if the newly opened chat has its own pause waiting.
  useEffect(() => {
    setPausedTurn(null)
    setExpandedTools({})
    setTurnError(null)
  }, [chatId])

  useEffect(() => {
    const pending = consumePendingPrompt(chatId)
    if (pending) sendRef.current(pending.prompt, pending.think, pending.images, pending.fileIds)
  }, [chatId])

  // `canContinue` is fetched by `useMessages` as soon as the chat opens — reopening a
  // chat that was left mid-turn (tab closed/navigated away before a confirmation was
  // answered) picks the pause back up once it comes back `true`.
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
      <Sidebar />
      <Div className="chat">
        <LazyList ref={lazyListRef} className="chat__list" threshold={LOAD_MORE_THRESHOLD} onTopReached={loadOlder}>
          {messages.map((m, i) => {
            const prev = messages[i - 1]
            const showDate = !prev || !isSameDay(new Date(m.created_at), new Date(prev.created_at))
            return (
              <Fragment key={i}>
                {showDate ? <DateSeparator date={new Date(m.created_at)} /> : null}
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
          {sending ? <PendingAssistantMessage /> : null}
        </LazyList>
        {pausedTurn ? <ToolConfirmation pending={pausedTurn.pending} onConfirm={handleConfirm} /> : null}
        {turnError ? (
          <Div className="chat__error">
            <Label className="chat__error-text" text={turnError} />
            <Button variant="secondary" text="Dismiss" onClicked={() => setTurnError(null)} />
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
}

export default Chat
