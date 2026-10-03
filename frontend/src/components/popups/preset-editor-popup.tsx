import {useState} from 'react'
import type {PresetIn} from '../../api/presets/types'
import {errorReason} from '../../utils/error-reason.ts'
import {numberText, parseNumber} from '../../utils/parse-number.ts'
import {Div, Input, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface PresetEditorPopupProps {
    open: boolean
    title: string
    /** What the fields start with each time the popup opens */
    initial: PresetIn
    /** Rejects with the backend's reason when the preset is refused */
    onSave: (preset: PresetIn) => Promise<void>
    onClose: () => void
}

const Field = ({label, hint, text, onChanged}: { label: string; hint: string; text: string; onChanged: (t: string) => void }) => (
    <Div className="field">
        <Label className="field__label" text={label}/>
        <Input text={text} onChanged={onChanged} placeholder="server default"/>
        <Label variant="secondary" className="field__help" text={hint}/>
    </Div>
)

const PresetForm = ({initial, onSave, onClose}: Omit<PresetEditorPopupProps, 'open' | 'title'>) => {
    const [name, setName] = useState(initial.name)
    const [temperature, setTemperature] = useState(numberText(initial.temperature))
    const [topP, setTopP] = useState(numberText(initial.top_p))
    const [topK, setTopK] = useState(numberText(initial.top_k))
    const [minP, setMinP] = useState(numberText(initial.min_p))
    const [repeat, setRepeat] = useState(numberText(initial.repeat_penalty))
    const [presence, setPresence] = useState(numberText(initial.presence_penalty))
    const [seed, setSeed] = useState(numberText(initial.seed))
    const [error, setError] = useState<string | null>(null)
    const [saving, setSaving] = useState(false)

    const save = async () => {
        setSaving(true)
        setError(null)
        try {
            await onSave({
                model_id: initial.model_id,
                name: name.trim(),
                temperature: parseNumber(temperature),
                top_p: parseNumber(topP),
                top_k: parseNumber(topK),
                min_p: parseNumber(minP),
                repeat_penalty: parseNumber(repeat),
                presence_penalty: parseNumber(presence),
                seed: parseNumber(seed),
            })
            onClose()
        } catch (e) {
            setError(errorReason(e, 'Could not save the preset.'))
        } finally {
            setSaving(false)
        }
    }

    return (
        <>
            <Div className="field">
                <Label className="field__label" text="Name"/>
                <Input text={name} onChanged={setName} placeholder="e.g. Coding"/>
            </Div>
            <Field label="Temperature" hint="0 to 5. Lower is more predictable, higher more varied." text={temperature} onChanged={setTemperature}/>
            <Field label="Top-p" hint="0 to 1. Only the likeliest words adding up to this share are considered." text={topP} onChanged={setTopP}/>
            <Field label="Top-k" hint="0 to 1000. Only this many likeliest words are considered; 0 turns it off." text={topK} onChanged={setTopK}/>
            <Field label="Min-p" hint="0 to 1. Drops words much less likely than the best one." text={minP} onChanged={setMinP}/>
            <Field label="Repeat penalty" hint="0 to 3. Above 1 discourages repeating itself." text={repeat} onChanged={setRepeat}/>
            <Field label="Presence penalty" hint="-2 to 2. Positive values push toward new topics." text={presence} onChanged={setPresence}/>
            <Field label="Seed" hint="A whole number makes replies repeatable (with temperature 0)." text={seed} onChanged={setSeed}/>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            <PopupActions confirmLabel={saving ? 'Saving…' : 'Save'} confirmDisabled={saving || !name.trim()}
                          onConfirm={() => void save()} onCancel={onClose} emphasis="confirm"/>
        </>
    )
}

/** Edits one sampling preset; a field left empty is not sent, so the server's own default applies */
export const PresetEditorPopup = ({open, title, initial, onSave, onClose}: PresetEditorPopupProps) => (
    <Popup open={open} onClose={onClose} title={title} width={480}>
        <PresetForm initial={initial} onSave={onSave} onClose={onClose}/>
    </Popup>
);
