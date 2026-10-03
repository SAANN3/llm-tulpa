import {useCallback, useEffect, useState} from 'react'
import '../styles/model-picker.scss'
import {listLocalFiles} from '../api/llm/local-files'
import type {LocalFile} from '../api/llm/types'
import {formatBytes} from '../utils/format-bytes.ts'
import {ModelFolderField} from './models/model-folder-field.tsx'
import {Button, Div, Label, Select} from './primitives'

const NO_PROJECTOR = 'no vision'

export interface LlamaPick {
    /** Relative to the model folder */
    file: string
    /** The vision projector it goes with, when it has one */
    projector: string | null
}

export interface LlamaModelPickProps {
    /** The files that are chosen, in the order they were chosen (the first is the default). The caller keeps
     * them, so they survive this list being mounted again. */
    picks: LlamaPick[]
    onChange: (picks: LlamaPick[]) => void
    onDownload: () => void
    /** Why the chosen models could not be added, when they could not */
    error: string | null
}

/** The `.gguf` files in the model folder: choose any number of them, each with its vision projector if
 * any. Nothing is written until the step is left. */
export const LlamaModelPick = ({picks, onChange, onDownload, error}: LlamaModelPickProps) => {
    const [files, setFiles] = useState<LocalFile[] | null>(null)
    const [configured, setConfigured] = useState(true)
    const [projectors, setProjectors] = useState<Record<string, string>>({})
    const [listError, setListError] = useState<string | null>(null)

    const reload = useCallback(() => {
        listLocalFiles().then((r) => {
            setConfigured(r.configured)
            setFiles(r.files.filter((f) => f.kind === 'model'))
        }).catch(() => setListError('Could not list the model folder.'))
    }, [])

    useEffect(reload, [reload])

    const projectorOf = (f: LocalFile) => picks.find((p) => p.file === f.path)?.projector ?? projectors[f.path] ?? f.suggested_projector ?? NO_PROJECTOR
    const asPick = (f: LocalFile, projector: string): LlamaPick => ({file: f.path, projector: projector === NO_PROJECTOR ? null : projector})

    const toggle = (f: LocalFile) => {
        if (picks.some((p) => p.file === f.path)) onChange(picks.filter((p) => p.file !== f.path))
        else onChange([...picks, asPick(f, projectorOf(f))])
    }

    const chooseProjector = (f: LocalFile, projector: string) => {
        setProjectors((p) => ({...p, [f.path]: projector}))
        if (picks.some((p) => p.file === f.path)) onChange(picks.map((p) => (p.file === f.path ? asPick(f, projector) : p)))
    }

    return (
        <Div className="setup__step">
            <Label className="setup__lead" text="Pick the models to add — any number of the .gguf files in your model folder. The first one you pick is your default."/>
            <ModelFolderField onChanged={reload}/>
            {configured ? <Button variant="secondary" text="Download a model" onClicked={onDownload}/> : null}
            {files?.length === 0 && configured ? <Label variant="secondary" className="field__help" text="No .gguf files in the model folder yet."/> : null}
            {files?.map((f) => {
                const index = picks.findIndex((p) => p.file === f.path)
                return (
                    <Div key={f.path} className={`model-picker__row model-picker__row--wrap${index >= 0 ? ' model-picker__row--active' : ''}`}>
                        <Div className="model-picker__info">
                            <Label className="model-picker__name" text={f.path}/>
                            <Label variant="secondary" className="model-picker__meta"
                                   text={[f.quantization, formatBytes(f.size_bytes), f.has_mtp ? 'has MTP head' : null, index === 0 ? 'default' : null].filter(Boolean).join(' · ')}/>
                        </Div>
                        {f.compatible_projectors.length > 0 ? (
                            <Select className="model-picker__projector" values={[NO_PROJECTOR, ...f.compatible_projectors]}
                                    selected={projectorOf(f)} onChosen={(v) => chooseProjector(f, v)}/>
                        ) : null}
                        <Button variant="secondary" text={index >= 0 ? 'Deselect' : 'Use'} onClicked={() => toggle(f)}/>
                    </Div>
                )
            })}
            {listError || error ? <Label className="model-picker__error" text={listError ?? error ?? ''}/> : null}
        </Div>
    )
}
