import {useEffect, useState} from 'react'
import '../../styles/system-prompt.scss'
import {getSystemPrompt} from '../../api/settings/system-prompt-get.ts'
import {setSystemPrompt} from '../../api/settings/system-prompt-set.ts'
import {Button, Div, Label, TextField} from '../../components/primitives'
import {useDocumentTitle} from '../../hooks/use-document-title.ts'
import {useGoBack} from '../../hooks/use-go-back.ts'

const SystemPrompt = () => {
    useDocumentTitle('System Prompt')
    const goBack = useGoBack('/settings/behaviour')
    const [loading, setLoading] = useState(true)
    const [defaultPrompt, setDefaultPrompt] = useState('')
    const [custom, setCustom] = useState<string | null>(null)
    const [draft, setDraft] = useState('')
    const [saving, setSaving] = useState(false)
    const [error, setError] = useState<string | null>(null)

    useEffect(() => {
        let cancelled = false
        setLoading(true)
        getSystemPrompt()
            .then(({custom, default: def}) => {
                if (cancelled) return
                setDefaultPrompt(def)
                setCustom(custom)
                setDraft(custom ?? '')
            })
            .catch(() => !cancelled && setError('Could not load the system prompt.'))
            .finally(() => !cancelled && setLoading(false))
        return () => {
            cancelled = true
        }
    }, [])

    if (loading) return null

    const dirty = draft.trim() !== (custom ?? '')

    const save = async (prompt: string | null) => {
        setSaving(true)
        setError(null)
        try {
            await setSystemPrompt(prompt)
            setCustom(prompt)
        } catch {
            setError('Saving failed.')
        } finally {
            setSaving(false)
        }
    }

    const onReset = () => {
        setDraft('')
        void save(null)
    }

    return (
        <Div className="page vbox system-prompt">
            <Div className="system-prompt__header">
                <Button variant="secondary" text="Back" onClicked={goBack}/>
                <Label className="section-heading" text="System prompt"/>
                <Label variant="secondary" className="system-prompt__status"
                       text={error ?? (saving ? 'Saving…' : custom ? 'Custom prompt active' : 'Built-in default active')}/>
                <Div className="system-prompt__spacer"/>
                <Button variant="secondary" text="Copy default" disabled={saving}
                        onClicked={() => setDraft(defaultPrompt)}/>
                <Button variant="secondary" text="Reset to default" disabled={saving} onClicked={onReset}/>
                <Button text="Save" disabled={saving || !dirty}
                        onClicked={() => void save(draft.trim() || null)}/>
            </Div>
            <Div className="system-prompt__panes">
                <Div className="system-prompt__pane">
                    <Label className="section-heading" text="Your prompt"/>
                    <TextField className="system-prompt__editor mono" text={draft} onChanged={setDraft}
                               placeholder="Leave empty to use the built-in default"/>
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
