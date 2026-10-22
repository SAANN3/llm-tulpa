import {useEffect, useState} from 'react'
import '../../styles/system-prompt.scss'
import {getSystemPrompt} from '../../api/settings/system-prompt-get.ts'
import {setSystemPrompt} from '../../api/settings/system-prompt-set.ts'
import {Button, Div, Label, TextField, ToggleSwitch} from '../../components/primitives'
import {SettingsRow} from '../../components/settings-fields.tsx'
import {useDocumentTitle} from '../../hooks/use-document-title.ts'
import {useGoBack} from '../../hooks/use-go-back.ts'

const SystemPrompt = () => {
    useDocumentTitle('System Prompt')
    const goBack = useGoBack('/settings/behaviour')
    const [loading, setLoading] = useState(true)
    const [defaultPrompt, setDefaultPrompt] = useState('')
    const [custom, setCustom] = useState<string | null>(null)
    const [append, setAppend] = useState(false)
    const [draft, setDraft] = useState('')
    const [draftAppend, setDraftAppend] = useState(false)
    const [saving, setSaving] = useState(false)
    const [error, setError] = useState<string | null>(null)

    useEffect(() => {
        let cancelled = false
        setLoading(true)
        getSystemPrompt()
            .then(({custom, append, default: def}) => {
                if (cancelled) return
                setDefaultPrompt(def)
                setCustom(custom)
                setAppend(append)
                setDraft(custom ?? '')
                setDraftAppend(append)
            })
            .catch(() => !cancelled && setError('Could not load the system prompt.'))
            .finally(() => !cancelled && setLoading(false))
        return () => {
            cancelled = true
        }
    }, [])

    if (loading) return null

    const dirty = draft.trim() !== (custom ?? '') || draftAppend !== append

    const save = async (prompt: string | null, add: boolean) => {
        setSaving(true)
        setError(null)
        try {
            await setSystemPrompt(prompt, add)
            setCustom(prompt)
            setAppend(add)
        } catch {
            setError('Saving failed.')
        } finally {
            setSaving(false)
        }
    }

    const onReset = () => {
        setDraft('')
        setDraftAppend(false)
        void save(null, false)
    }

    return (
        <Div className="page vbox system-prompt">
            <Div className="system-prompt__header">
                <Div className="system-prompt__header-bar">
                    <Button className="system-prompt__back" variant="secondary" text="Back" onClicked={goBack}/>
                    <Label className="section-heading" text="System prompt"/>
                    <Label variant="secondary" className="system-prompt__status"
                           text={error ?? (saving ? 'Saving…' : custom ? (append ? 'Your rules are added to the built-in prompt' : 'Custom prompt active') : 'Built-in default active')}/>
                </Div>
                <Div className="system-prompt__spacer"/>
                <Div className="system-prompt__actions">
                    <Button className="system-prompt__action" variant="secondary" text="Copy default" disabled={saving || draftAppend}
                            onClicked={() => setDraft(defaultPrompt)}/>
                    <Button className="system-prompt__action" variant="secondary" text="Reset to default" disabled={saving} onClicked={onReset}/>
                    <Button className="system-prompt__action" text="Save" disabled={saving || !dirty}
                            onClicked={() => void save(draft.trim() || null, draftAppend)}/>
                </Div>
            </Div>
            <SettingsRow label="Add to the built-in prompt"
                         help="On: your text follows the built-in prompt, so its tool rules stay. Off: your text replaces it. A change makes the next message read the whole chat again once.">
                <ToggleSwitch toggled={draftAppend} onToggled={setDraftAppend} disabled={saving}/>
            </SettingsRow>
            <Div className="system-prompt__panes">
                <Div className="system-prompt__pane">
                    <Label className="section-heading" text={draftAppend ? 'Your rules' : 'Your prompt'}/>
                    <TextField className="system-prompt__editor mono" text={draft} onChanged={setDraft}
                               placeholder={draftAppend ? 'Rules to add after the built-in prompt' : 'Leave empty to use the built-in default'}/>
                </Div>
                <Div className="system-prompt__pane">
                    <Label className="section-heading" text="Default prompt"/>
                    <pre className="system-prompt__default mono">{defaultPrompt}</pre>
                </Div>
            </Div>
        </Div>
    )
};

export default SystemPrompt
