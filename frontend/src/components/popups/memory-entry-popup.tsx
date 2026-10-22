import {useState} from 'react'
import '../../styles/memory.scss'
import type {MemoryCategory, MemoryEntryRequest} from '../../api/memory/types.ts'
import {MEMORY_CATEGORIES} from '../../utils/memory-categories.ts'
import {ChoiceGroup, Div, Input, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface MemoryEntryPopupProps {
    open: boolean
    title: string
    /** What the form starts with each time the popup opens */
    initial?: MemoryEntryRequest
    onSubmit: (entry: MemoryEntryRequest) => void
    onClose: () => void
}

// Its own component so the draft lives only while the popup is open: every opening starts again from `initial`
const EntryForm = ({initial, onSubmit, onClose}: Omit<MemoryEntryPopupProps, 'open' | 'title'>) => {
    const [category, setCategory] = useState<MemoryCategory>(initial?.category ?? 'preferences')
    const [text, setText] = useState(initial?.text ?? '')
    const trimmed = text.trim()
    const submit = () => {
        if (!trimmed) return
        onClose()
        onSubmit({category, text: trimmed})
    }

    return (
        <>
            <ChoiceGroup label="What the entry is about" chosen={category} onChosen={(value) => setCategory(value as MemoryCategory)}
                         options={MEMORY_CATEGORIES}/>
            <Div className="memory-entry__field">
                <Label variant="secondary" text="What to remember"/>
                <Input text={text} onChanged={setText} placeholder="A short note, in your own words" autoFocus
                       onKeyDown={(e) => e.key === 'Enter' && submit()}/>
            </Div>
            <PopupActions confirmLabel="Save" confirmDisabled={!trimmed} onConfirm={submit} onCancel={onClose}/>
        </>
    )
};

/** Writes or changes an entry of the user's memory: what it is about, and the sentence */
export const MemoryEntryPopup = ({open, title, initial, onSubmit, onClose}: MemoryEntryPopupProps) => (
    <Popup open={open} onClose={onClose} title={title} width={520}
           actions={[{keys: ['Enter'], shown: 'enter', label: 'save'}]}>
        <EntryForm initial={initial} onSubmit={onSubmit} onClose={onClose}/>
    </Popup>
);
