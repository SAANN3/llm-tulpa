import {useLayoutEffect, useRef, useState} from 'react'
import type {KeyboardEvent} from 'react'
import {Close} from 'pixelarticons/react'
import type {ChatMemoryOut} from '../../api/chats/types.ts'
import {Button, Div, TextField} from '../../components/primitives'

export interface MemoryDraft {
    facts: string[]
    summary: string
    notes: string
}

// Escape in a text field only leaves the field, so the tab keys work again; a second Escape goes back to the chat
const leaveOnEscape = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Escape') e.currentTarget.blur()
}

/** One key fact as a text field that grows with its text, so a long fact reads as a paragraph, not a scrolled line */
const FactRow = ({text, readOnly, focus, onChanged, onRemove, onEnter}: {
    text: string
    readOnly: boolean
    /** Changes when the row should take the focus (it was just added); null for no claim */
    focus: number | null
    onChanged: (text: string) => void
    onRemove: () => void
    onEnter: () => void
}) => {
    const ref = useRef<HTMLTextAreaElement>(null)
    useLayoutEffect(() => {
        const field = ref.current
        if (!field) return
        field.style.height = '0'
        field.style.height = `${field.scrollHeight}px`
    }, [text])
    useLayoutEffect(() => {
        if (focus != null) ref.current?.focus()
    }, [focus])
    return (
        <Div className="chat-info__fact">
            <TextField ref={ref} className="chat-info__fact-text" text={text} onChanged={(value) => onChanged(value.replace(/\n/g, ' '))}
                       disabled={readOnly} placeholder="A fact the model should keep"
                       onKeyDown={(e) => {
                           leaveOnEscape(e)
                           // A fact is one line: Enter starts the next one
                           if (e.key === 'Enter') {
                               e.preventDefault()
                               onEnter()
                           }
                       }}/>
            {readOnly ? null : (
                <Button variant="secondary" className="chat-info__fact-remove" title="Remove this fact" onClicked={onRemove}>
                    <Close width={14} height={14}/>
                </Button>
            )}
        </Div>
    )
};

/** What the chat remembers past a fold, editable: the key facts (the goal is the summarizer's), the summary and the
 * model's notes. Nothing is sent until Save. */
export const MemoryTab = ({memory, folded_messages, draft, onDraft, readOnly}: {
    memory: ChatMemoryOut
    folded_messages: number
    draft: MemoryDraft
    onDraft: (draft: MemoryDraft) => void
    readOnly: boolean
}) => {
    // The fact just added takes the focus; `n` makes a second add at the same place a new claim
    const [focus, setFocus] = useState<{index: number, n: number} | null>(null)
    const setFacts = (facts: string[]) => onDraft({...draft, facts})
    const addFact = (at = draft.facts.length) => {
        setFocus((prev) => ({index: at, n: (prev?.n ?? 0) + 1}))
        setFacts([...draft.facts.slice(0, at), '', ...draft.facts.slice(at)])
    }

    return (
        <Div className="chat-info__stack">
            <Div className="chat-info__pane">
                <Div className="chat-info__pane-title">
                    <span>Goal &amp; key facts</span>
                    <span className="chat-info__spacer"/>
                    {memory.folded && !readOnly ? <Button variant="secondary" className="chat-info__small" text="+ Add a fact" onClicked={() => addFact()}/> : null}
                </Div>
                {memory.folded ? (
                    <>
                        <Div className="chat-info__dim">{memory.goal ? `Goal: ${memory.goal}` : 'No goal was set.'}</Div>
                        <Div className="chat-info__facts">
                            {draft.facts.map((fact, i) => (
                                <FactRow key={i} text={fact} readOnly={readOnly} focus={focus?.index === i ? focus.n : null}
                                         onChanged={(text) => setFacts(draft.facts.map((f, j) => (j === i ? text : f)))}
                                         onRemove={() => setFacts(draft.facts.filter((_, j) => j !== i))}
                                         onEnter={() => addFact(i + 1)}/>
                            ))}
                            {draft.facts.length === 0 ? <Div className="chat-info__dim">No key facts.</Div> : null}
                        </Div>
                    </>
                ) : (
                    <Div className="chat-info__dim">
                        The chat hasn't been folded yet: the goal and key facts are written when its oldest part is summarized.
                    </Div>
                )}
            </Div>
            <Div className="chat-info__pane chat-info__pane--grow">
                <Div className="chat-info__pane-title">
                    <span>Summary</span>
                    <span className="chat-info__spacer"/>
                    {memory.folded ? <span className="chat-info__dim">{`of the first ${folded_messages} messages`}</span> : null}
                </Div>
                {memory.folded ? (
                    <TextField className="chat-info__editor" text={draft.summary} disabled={readOnly}
                               onChanged={(summary) => onDraft({...draft, summary})} onKeyDown={leaveOnEscape}/>
                ) : (
                    <Div className="chat-info__dim">Nothing is summarized yet.</Div>
                )}
            </Div>
            <Div className="chat-info__pane chat-info__pane--grow">
                <Div className="chat-info__pane-title">
                    <span>Notes</span>
                    <span className="chat-info__spacer"/>
                    <span className="chat-info__dim">
                        {memory.notes_pending ? "the model's own; these newer ones reach it at the next fold" : "the model's own"}
                    </span>
                </Div>
                <TextField className="chat-info__editor" text={draft.notes} disabled={readOnly}
                           placeholder="The model hasn't written any notes"
                           onChanged={(notes) => onDraft({...draft, notes})} onKeyDown={leaveOnEscape}/>
            </Div>
        </Div>
    )
};
