import {useEffect, useState} from 'react'
import {getOllamaSettings, setOllamaSettings, testOllama, type OllamaSettings} from '../api/llm/ollama'
import {errorReason} from '../utils/error-reason.ts'
import {numberText, parseNumber} from '../utils/parse-number.ts'
import {Button, Div, Input, Label} from './primitives'

export interface OllamaAddressFieldProps {
    /** Called after the address was saved, so a list of Ollama's models can be read again */
    onSaved?: () => void
}

/** Where the backend reaches Ollama, with a button to check an address before it is saved. Owner only. */
export const OllamaAddressField = ({onSaved}: OllamaAddressFieldProps) => {
    const [saved, setSaved] = useState<OllamaSettings | null>(null)
    const [url, setUrl] = useState('')
    const [context, setContext] = useState('')
    const [note, setNote] = useState<{ text: string; bad?: boolean } | null>(null)

    const show = (s: OllamaSettings) => {
        setSaved(s)
        setUrl(s.url)
        setContext(numberText(s.context_length))
    }

    useEffect(() => {
        getOllamaSettings().then(show).catch(() => setNote({text: 'Could not read the Ollama settings.', bad: true}))
    }, [])

    if (!saved) return note ? <Label variant="secondary" className="model-picker__error" text={note.text}/> : null

    const next: OllamaSettings = {url: url.trim(), context_length: parseNumber(context) ?? saved.context_length}
    const changed = next.url !== saved.url || next.context_length !== saved.context_length

    const test = async () => {
        setNote(null)
        try {
            const result = await testOllama(next.url)
            setNote(result.reachable
                ? {text: `Reachable: ${result.models} model${result.models === 1 ? '' : 's'} installed.`}
                : {text: `Not reachable: ${result.reason}`, bad: true})
        } catch (e) {
            setNote({text: errorReason(e, 'Could not test that address.'), bad: true})
        }
    }

    const save = async () => {
        setNote(null)
        try {
            show(await setOllamaSettings(next))
            setNote({text: 'Saved.'})
            onSaved?.()
        } catch (e) {
            setNote({text: errorReason(e, 'Could not save.'), bad: true})
        }
    }

    return (
        <Div className="field">
            <Label className="field__label" text="Ollama address"/>
            <Input text={url} onChanged={setUrl} placeholder="http://localhost:11434"/>
            <Label className="field__label" text="Context window (tokens)"/>
            <Input text={context} onChanged={setContext}/>
            <Label variant="secondary" className="field__help"
                   text="The address applies at once. The context window must match Ollama's own setting (OLLAMA_CONTEXT_LENGTH) and applies from the next start."/>
            <Div className="popup__actions">
                <Button variant="secondary" text="Test" disabled={!next.url} onClicked={() => void test()}/>
                <Button text="Save" disabled={!changed} onClicked={() => void save()}/>
            </Div>
            {note ? <Label variant="secondary" className={note.bad ? 'model-picker__error' : 'model-picker__meta'} text={note.text}/> : null}
        </Div>
    )
};
