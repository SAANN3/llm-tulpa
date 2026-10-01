import {useState} from 'react'
import {Input} from '../../primitives'
import {Popup} from './popup.tsx'
import {PopupActions} from './popup-actions.tsx'

export interface InputPopupProps {
    open: boolean
    title: string
    /** What the field starts with each time the popup opens */
    value?: string
    placeholder?: string
    confirmLabel?: string
    /** Runs after the popup has closed itself, with the trimmed, non-empty text */
    onSubmit: (text: string) => void
    onClose: () => void
}

// Its own component so the draft lives only while the popup is open: the popup's children
// are unmounted when it closes, so every opening starts again from `value`.
const InputForm = ({value = '', placeholder, confirmLabel = 'Save', onSubmit, onClose}: Omit<InputPopupProps, 'open' | 'title'>) => {
    const [draft, setDraft] = useState(value)
    const text = draft.trim()

    return (
        <>
            <Input text={draft} onChanged={setDraft} placeholder={placeholder}/>
            <PopupActions
                confirmLabel={confirmLabel}
                confirmDisabled={!text}
                onConfirm={() => {
                    onClose()
                    onSubmit(text)
                }}
                onCancel={onClose}
            />
        </>
    )
};

/** Asks for one line of text: a rename, a new name, a short answer */
export const InputPopup = ({open, title, value, placeholder, confirmLabel, onSubmit, onClose}: InputPopupProps) => (
    <Popup open={open} onClose={onClose} title={title}>
        <InputForm value={value} placeholder={placeholder} confirmLabel={confirmLabel} onSubmit={onSubmit} onClose={onClose}/>
    </Popup>
);
