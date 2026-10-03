import {useEffect, useState} from 'react'
import '../styles/model-picker.scss'
import {listLocalFiles} from '../api/llm/local-files'
import type {LocalFile} from '../api/llm/types'
import {registerModel} from '../api/runtime/register'
import {errorReason} from '../utils/error-reason.ts'
import {formatBytes} from '../utils/format-bytes.ts'
import {Button, Div, Label, Select} from './primitives'

const NO_PROJECTOR = 'no vision'

/** The `.gguf` files in the model folder: pick one to register it as a model, with its vision projector if any */
export const LlamaModelPick = ({onPicked}: { onPicked: (file: string, profileId: number) => void }) => {
    const [files, setFiles] = useState<LocalFile[] | null>(null)
    const [configured, setConfigured] = useState(true)
    const [picked, setPicked] = useState<string | null>(null)
    const [projectors, setProjectors] = useState<Record<string, string>>({})
    const [error, setError] = useState<string | null>(null)

    useEffect(() => {
        listLocalFiles().then((r) => {
            setConfigured(r.configured)
            setFiles(r.files.filter((f) => f.kind === 'model'))
        }).catch(() => setError('Could not list the model folder.'))
    }, [])

    const choose = async (file: LocalFile) => {
        setError(null)
        const projector = projectors[file.path] ?? file.suggested_projector ?? NO_PROJECTOR
        try {
            const model = await registerModel(file.path, undefined, projector === NO_PROJECTOR ? undefined : projector)
            setPicked(file.path)
            onPicked(file.path, model.profile_ids[0])
        } catch (e) {
            setError(errorReason(e, 'Could not add that model.'))
        }
    }

    return (
        <Div className="setup__step">
            <Label className="setup__lead" text="Pick the model to chat with — one of the .gguf files in your model folder."/>
            {!configured ? <Label variant="secondary" className="field__help" text="No model folder is set: add model_dir to the backend's settings.json."/> : null}
            {files?.length === 0 && configured ? <Label variant="secondary" className="field__help" text="No .gguf files in the model folder yet."/> : null}
            {files?.map((f) => (
                <Div key={f.path} className={`model-picker__row${picked === f.path ? ' model-picker__row--active' : ''}`}>
                    <Div className="model-picker__info">
                        <Label className="model-picker__name" text={f.path}/>
                        <Label variant="secondary" className="model-picker__meta"
                               text={[f.quantization, formatBytes(f.size_bytes), f.has_mtp ? 'has MTP head' : null].filter(Boolean).join(' · ')}/>
                    </Div>
                    {f.compatible_projectors.length > 0 ? (
                        <Select values={[NO_PROJECTOR, ...f.compatible_projectors]}
                                selected={projectors[f.path] ?? f.suggested_projector ?? NO_PROJECTOR}
                                onChosen={(v) => setProjectors((p) => ({...p, [f.path]: v}))}/>
                    ) : null}
                    {picked === f.path ? <Label variant="secondary" className="model-picker__active-tag" text="selected"/> :
                        <Button variant="secondary" text="Use" onClicked={() => void choose(f)}/>}
                </Div>
            ))}
            {error ? <Label className="model-picker__error" text={error}/> : null}
        </Div>
    )
}
