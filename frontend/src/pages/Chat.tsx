import { useEffect, useRef, useState } from 'react'
import { Navigate, useSearchParams } from 'react-router-dom'

import { getChats } from '../api/chats/get'
import { ChatMessage } from '../components/ChatMessage'
import type { LazyListHandle } from '../components/LazyList'
import { LazyList } from '../components/LazyList'
import { PendingAssistantMessage } from '../components/PendingAssistantMessage'
import { Div } from '../components/primitives'
import { Sidebar } from '../components/Sidebar'
import { ToolConfirmation } from '../components/ToolConfirmation'
import { ToolMessage } from '../components/ToolMessage'
import { UserInput } from '../components/UserInput'
import { useDocumentTitle } from '../hooks/useDocumentTitle'
import type { Decisions, PendingConfirmations, TurnResult } from '../hooks/useMessages'
import { useMessages } from '../hooks/useMessages'
import { consumePendingPrompt, peekPendingPrompt } from '../pendingPrompt'

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

  // Set whenever a turn pauses on a call needing a decision — cleared once `confirm`
  // resolves to a `TurnResult` that doesn't need one (possibly after several rounds, if
  // the model's next reply asks for more tools).
  const [pausedTurn, setPausedTurn] = useState<PausedTurn | null>(null)

  // `ChatView` is reused across a chat switch (only `chatId` changes), so a turn kicked
  // off for the old chat can still be running when its result comes back — checked
  // against this before touching `pausedTurn`, so a slow reply from a chat that's no
  // longer open can't pop the confirmation panel over whatever's open now.
  const chatIdRef = useRef(chatId)
  chatIdRef.current = chatId

  const handleTurnResult = (forChatId: number, result: TurnResult) => {
    if (chatIdRef.current !== forChatId) return
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
    handleTurnResult(forChatId, await confirm(decisions))
  }
  const handleSend = async (prompt: string, think?: boolean) => {
    const forChatId = chatId
    handleTurnResult(forChatId, await send(prompt, think))
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
  const [initialThink] = useState(() => peekPendingPrompt(chatId)?.think ?? true)

  // Switching chats reuses this same `ChatView` instance (just a new `chatId` prop), so
  // a pause left over from the chat just navigated away from would otherwise still be
  // showing here — clear it immediately; the `canContinue` effect below repopulates it
  // if the newly opened chat has its own pause waiting.
  useEffect(() => {
    setPausedTurn(null)
  }, [chatId])

  useEffect(() => {
    const pending = consumePendingPrompt(chatId)
    if (pending) sendRef.current(pending.prompt, pending.think)
  }, [chatId])

  // `canContinue` is fetched by `useMessages` as soon as the chat opens — reopening a
  // chat that was left mid-turn (tab closed/navigated away before a confirmation was
  // answered) picks the pause back up once it comes back `true`.
  useEffect(() => {
    if (!canContinue) return
    const forChatId = chatId

    resumeRef.current().then((result) => {
      if (result) handleTurnResult(forChatId, result)
    })
  }, [canContinue, chatId])

  return (
    <Div className="page">
      <Sidebar />
      <Div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', padding: 8, gap: 8 }}>
        <LazyList ref={lazyListRef} style={{ flex: 1, minHeight: 0 }} threshold={LOAD_MORE_THRESHOLD} onTopReached={loadOlder}>
          {messages.map((m, i) =>
            m.role === 'tool' ? (
              <ToolMessage
                key={i}
                tool_name={m.tool_name ?? m.role}
                content={m.content}
                created_at={m.created_at}
                arguments={m.arguments}
              />
            ) : (
              <ChatMessage
                key={i}
                role={m.role}
                content={m.content}
                created_at={m.created_at}
                thinking={m.thinking}
                thought_duration_ms={m.thought_duration_ms}
              />
            ),
          )}
          {sending ? <PendingAssistantMessage /> : null}
        </LazyList>
        {pausedTurn ? <ToolConfirmation pending={pausedTurn.pending} onConfirm={handleConfirm} /> : null}
        <UserInput blocked={sending || pausedTurn != null} onSended={handleSend} inputDisabled={false} initialThink={initialThink} />
      </Div>
    </Div>
  )
}

export default Chat
