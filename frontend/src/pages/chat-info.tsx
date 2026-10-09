import {useCallback, useEffect, useState} from 'react'
import {Navigate, useNavigate, useParams} from 'react-router-dom'
import '../styles/chat-info.scss'
import {getChatContext} from '../api/chats/context.ts'
import {editChatFacts} from '../api/chats/edit-facts.ts'
import {editChatNotes} from '../api/chats/edit-notes.ts'
import {editChatSummary} from '../api/chats/edit-summary.ts'
import {getChats} from '../api/chats/get.ts'
import type {ChatContextResponse, ChatOut} from '../api/chats/types.ts'
import {Frame} from '../components/frame.tsx'
import {Button, Div, Label} from '../components/primitives'
import {useRuns} from '../context/use-runs.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useGoBack} from '../hooks/use-go-back.ts'
import {errorReason} from '../utils/error-reason.ts'
import {ContextTab} from './chat-info/context-tab.tsx'
import {MemoryTab, type MemoryDraft} from './chat-info/memory-tab.tsx'

const TABS = [{id: 'context', label: 'Context'}, {id: 'memory', label: 'Memory'}]

const draftOf = (context: ChatContextResponse): MemoryDraft => ({
    facts: context.memory.facts,
    summary: context.memory.summary ?? '',
    notes: context.memory.notes ?? '',
})

const sameFacts = (a: string[], b: string[]) => {
    const clean = (facts: string[]) => facts.map((fact) => fact.trim()).filter(Boolean)
    return JSON.stringify(clean(a)) === JSON.stringify(clean(b))
}

/** A chat's context, part by part, and what it remembers past a fold (key facts, summary, notes), which the user can
 * correct. The tab lives in the address (`/chat/12/info/memory`). */
const ChatInfo = () => {
    const {id, tab: tabSlug} = useParams()
    const chatId = Number(id)
    if (!Number.isInteger(chatId)) return <Navigate to="/" replace/>
    return <ChatInfoView key={chatId} chatId={chatId} tab={TABS.find((t) => t.id === tabSlug)?.id ?? 'context'}/>
};

const ChatInfoView = ({chatId, tab}: { chatId: number, tab: string }) => {
    useDocumentTitle('Chat info')
    const navigate = useNavigate()
    const goBack = useGoBack(`/chat?id=${chatId}`)
    const {working} = useRuns()
    const running = working.has(chatId)
    const [chat, setChat] = useState<ChatOut | null>(null)
    const [context, setContext] = useState<ChatContextResponse | null>(null)
    const [draft, setDraft] = useState<MemoryDraft | null>(null)
    const [saving, setSaving] = useState(false)
    const [status, setStatus] = useState<string | null>(null)

    const load = useCallback(async () => {
        const [chats, next] = await Promise.all([getChats({id: chatId}), getChatContext(chatId)])
        if ('chats' in chats) return
        setChat(chats)
        setContext(next)
        setDraft(draftOf(next))
    }, [chatId])

    useEffect(() => {
        load().catch((e) => setStatus(errorReason(e, 'Could not load the chat.')))
    }, [load])

    // A run that ends changes the context (and maybe, with a fold, the memory): read it again, unless the user is
    // in the middle of changing the memory
    const [wasRunning, setWasRunning] = useState(running)
    if (wasRunning !== running) {
        setWasRunning(running)
        if (!running && context && draft && !isDirty(context, draft)) void load().catch(() => undefined)
    }

    const dirty = context != null && draft != null && isDirty(context, draft)

    const save = async () => {
        if (!context || !draft) return
        setSaving(true)
        setStatus(null)
        try {
            const memory = context.memory
            if (!sameFacts(draft.facts, memory.facts)) await editChatFacts(chatId, draft.facts)
            if (draft.summary !== (memory.summary ?? '')) await editChatSummary(chatId, draft.summary)
            if (draft.notes !== (memory.notes ?? '')) await editChatNotes(chatId, draft.notes)
            await load()
            setStatus('Saved; the model gets it from its next turn')
        } catch (e) {
            setStatus(errorReason(e, 'Saving failed.'))
        } finally {
            setSaving(false)
        }
    }

    const showTab = (next: string) => navigate(`/chat/${chatId}/info/${next}`, {replace: true})

    const detail = chat ? `${chat.name} · ${chat.model}` : ''
    return (
        <Div className="page vbox chat-info">
            <Div className="chat-info__header">
                <Button variant="secondary" text="Back" onClicked={goBack}/>
                <Label className="section-heading" text="Chat info"/>
                <Label variant="secondary" className="chat-info__status"
                       text={status ?? (running ? `${detail} · working, the memory can't be changed until it stops` : detail)}/>
                <Div className="chat-info__spacer"/>
                <Button text={saving ? 'Saving…' : 'Save'} disabled={saving || running || !dirty} onClicked={() => void save()}/>
            </Div>
            <Frame className="chat-info__panel" bodyClassName="chat-info__body" tabs={TABS} activeTab={tab} onTab={showTab}
                   actions={tab === 'memory' ? [{keys: [], shown: 'esc', label: 'leave a text field'}] : []}
                   onEscape={goBack} escapeLabel="back">
                {context && chat && draft ? (
                    tab === 'memory'
                        ? <MemoryTab memory={context.memory} folded_messages={context.folded_messages} draft={draft}
                                     onDraft={setDraft} readOnly={running || saving}/>
                        : <ContextTab chat={chat} context={context}/>
                ) : null}
            </Frame>
        </Div>
    )
};

const isDirty = (context: ChatContextResponse, draft: MemoryDraft) =>
    !sameFacts(draft.facts, context.memory.facts)
    || draft.summary !== (context.memory.summary ?? '')
    || draft.notes !== (context.memory.notes ?? '')

export default ChatInfo
