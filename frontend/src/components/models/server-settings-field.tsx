import {useEffect, useState} from 'react'
import {getServerSettings, setServerSettings, type ServerSettings} from '../../api/runtime/server-settings'
import {errorReason} from '../../utils/error-reason.ts'
import {numberText, parseNumber} from '../../utils/parse-number.ts'
import {Button, Div, Input, Label, ToggleSwitch} from '../primitives'

/** How the model server behaves over time: when it unloads, whether it loads at start, how long a load may
 * take. Owner only. */
export const ServerSettingsField = () => {
    const [saved, setSaved] = useState<ServerSettings | null>(null)
    const [idle, setIdle] = useState('')
    const [autostart, setAutostart] = useState(false)
    const [timeout, setTimeoutText] = useState('')
    const [note, setNote] = useState<{ text: string; bad?: boolean } | null>(null)

    const show = (s: ServerSettings) => {
        setSaved(s)
        setIdle(numberText(s.idle_unload_minutes))
        setAutostart(s.autostart)
        setTimeoutText(numberText(s.load_timeout_secs))
    }

    useEffect(() => {
        getServerSettings().then(show).catch(() => setNote({text: 'Could not read the server settings.', bad: true}))
    }, [])

    if (!saved) return note ? <Label variant="secondary" className="models__error" text={note.text}/> : null

    const next: ServerSettings = {
        idle_unload_minutes: parseNumber(idle) ?? 0,
        autostart,
        load_timeout_secs: parseNumber(timeout) ?? saved.load_timeout_secs,
    }
    const changed = JSON.stringify(next) !== JSON.stringify(saved)

    const save = async () => {
        setNote(null)
        try {
            show(await setServerSettings(next))
            setNote({text: 'Saved.'})
        } catch (e) {
            setNote({text: errorReason(e, 'Could not save.'), bad: true})
        }
    }

    return (
        <Div className="models__section">
            <Label variant="secondary" className="models__heading" text="Model server"/>
            <Div className="models__fields">
                <Div className="field">
                    <Label className="field__label" text="Unload after idle minutes"/>
                    <Input text={idle} onChanged={setIdle} placeholder="0 never"/>
                </Div>
                <Div className="field">
                    <Label className="field__label" text="Load timeout (seconds)"/>
                    <Input text={timeout} onChanged={setTimeoutText}/>
                </Div>
            </Div>
            <Label variant="secondary" className="field__help"
                   text="An idle model is unloaded after this long, which frees the GPU for something else (the next chat loads it again, a few seconds when it is cached). 0 keeps it loaded."/>
            <Div className="field__row">
                <Label className="field__row-label field__row-label--wide" text="Load the default model when the backend starts"/>
                <ToggleSwitch toggled={autostart} onToggled={setAutostart}/>
            </Div>
            <Div className="models__toolbar">
                <Button text="Save" disabled={!changed} onClicked={() => void save()}/>
                {note ? <Label variant="secondary" className={note.bad ? 'models__error' : 'models__meta'} text={note.text}/> : null}
            </Div>
        </Div>
    )
};
