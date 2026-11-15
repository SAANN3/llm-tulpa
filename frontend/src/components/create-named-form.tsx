import {useRef, useState} from 'react'

import '../styles/create-named-form.scss'
import {ChoiceGroup, Div, Input, Label} from './primitives'
import {PopupActions} from './popups/base/popup-actions.tsx'

export interface CreateNamedFormProps<T> {
    resource: string
    create: (name: string) => Promise<T>
    generateName: (description: string) => Promise<string>
    onCreated: (created: T) => void
    onCancel: () => void
}

type CreateMode = 'describe' | 'name'

/** Shared by folder and blueprint creation, including the folder picker's inline form. */
export const CreateNamedForm = <T,>({resource, create, generateName, onCreated, onCancel}: CreateNamedFormProps<T>) => {
    const [mode, setMode] = useState<CreateMode>('describe')
    const [draft, setDraft] = useState('')
    const [saving, setSaving] = useState(false)
    const inFlight = useRef(false)
    const [error, setError] = useState<string | null>(null)
    const inputRef = useRef<HTMLInputElement>(null)

    const switchMode = (newMode: CreateMode) => {
        if (inFlight.current) return
        setMode(newMode)
        inputRef.current?.focus()
    }

    const onCreate = async () => {
        const text = draft.trim()
        if (!text || inFlight.current) return

        inFlight.current = true
        setSaving(true)

        setError(null)
        try {
            let name = text
            if (mode === 'describe') {
                name = (await generateName(text)).trim() || text
            }
            onCreated(await create(name))
        } catch {
            setError(`Could not create the ${resource}.`)
        } finally {
            inFlight.current = false
            setSaving(false)
        }
    }

    return (
        <Div className="vbox create-named-form">
            <ChoiceGroup className="create-named-form__mode" label={`How to name the ${resource}`} chosen={mode}
                         onChosen={(chosen) => switchMode(chosen as CreateMode)}
                         options={[{value: 'describe', label: 'Describe it'}, {value: 'name', label: 'Write a name'}]}/>
            <Input
                inputRef={inputRef}
                autoFocus
                disabled={saving}
                text={draft}
                onChanged={setDraft}
                placeholder={mode === 'describe' ? `What is this ${resource} for?` : `${resource[0].toUpperCase()}${resource.slice(1)} name`}
                onKeyDown={(e) => e.key === 'Enter' && void onCreate()}
            />
            {error
                ? <Label variant="secondary" className="create-named-form__error" text={error}/>
                : mode === 'describe' ? <Label variant="secondary" className="field__help" text="The model names it from your description."/> : null}
            <PopupActions
                emphasis="confirm"
                confirmLabel={saving ? (mode === 'describe' ? 'Thinking…' : 'Creating…') : 'Create'}
                confirmDisabled={!draft.trim() || saving}
                onConfirm={onCreate}
                onCancel={onCancel}
            />
        </Div>
    )
};
