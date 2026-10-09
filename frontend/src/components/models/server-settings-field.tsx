import {useEffect, useRef, useState} from 'react'
import {getServerSettings, setServerSettings, type ServerSettings} from '../../api/runtime/server-settings'
import {errorReason} from '../../utils/error-reason.ts'
import {numberText, parseNumber} from '../../utils/parse-number.ts'
import {Div, Input, Label, ToggleSwitch} from '../primitives'
import {SettingsRow} from '../settings-fields.tsx'

type Field = keyof ServerSettings

/** How the model server behaves over time: when it unloads, whether it loads at start, how long a load may
 * take. Each applies as it is changed (a switch at once, a number when its field is left), with a short "saved"
 * next to it, so there is no Save button to forget. Owner only. */
export const ServerSettingsField = () => {
    const [saved, setSaved] = useState<ServerSettings | null>(null)
    const [idle, setIdle] = useState('')
    const [timeout, setTimeoutText] = useState('')
    const [done, setDone] = useState<Field | null>(null)
    const [error, setError] = useState<string | null>(null)
    const doneTimer = useRef<number>(undefined)
    useEffect(() => () => window.clearTimeout(doneTimer.current), [])

    const show = (s: ServerSettings) => {
        setSaved(s)
        setIdle(numberText(s.idle_unload_minutes))
        setTimeoutText(numberText(s.load_timeout_secs))
    }

    useEffect(() => {
        getServerSettings().then(show).catch(() => setError('Could not read the server settings.'))
    }, [])

    if (!saved) return error ? <Label variant="secondary" className="models__error" text={error}/> : null

    const save = async (field: Field, change: Partial<ServerSettings>) => {
        const next = {...saved, ...change}
        if (JSON.stringify(next) === JSON.stringify(saved)) return
        setError(null)
        try {
            show(await setServerSettings(next))
            setDone(field)
            window.clearTimeout(doneTimer.current)
            doneTimer.current = window.setTimeout(() => setDone(null), 1500)
        } catch (e) {
            setError(errorReason(e, 'Could not save.'))
            show(saved)
        }
    }

    const savedMark = (field: Field) => (done === field ? <Label variant="secondary" className="models__saved" text="saved"/> : null)

    return (
        <Div className="models__section">
            <Label variant="secondary" className="models__heading" text="Model server"/>
            <SettingsRow label="Unload after idle minutes"
                         help="Frees the GPU for something else; the next chat loads it again (a few seconds when it is cached). 0 keeps it loaded.">
                {savedMark('idle_unload_minutes')}
                <Input className="models__number" text={idle} onChanged={setIdle} placeholder="0"
                       onBlur={() => void save('idle_unload_minutes', {idle_unload_minutes: parseNumber(idle) ?? 0})}/>
            </SettingsRow>
            <SettingsRow label="Load timeout" help="How long a load may take before it counts as failed, in seconds.">
                {savedMark('load_timeout_secs')}
                <Input className="models__number" text={timeout} onChanged={setTimeoutText}
                       onBlur={() => void save('load_timeout_secs', {load_timeout_secs: parseNumber(timeout) ?? saved.load_timeout_secs})}/>
            </SettingsRow>
            <SettingsRow label="Load the default model when the backend starts">
                {savedMark('autostart')}
                <ToggleSwitch toggled={saved.autostart} onToggled={(autostart) => void save('autostart', {autostart})}/>
            </SettingsRow>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
        </Div>
    )
};
