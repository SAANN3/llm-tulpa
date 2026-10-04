import {useCallback, useEffect, useRef, useState} from 'react'
import {choosePreset} from '../../api/presets/choose'
import {createPreset} from '../../api/presets/create'
import {deletePreset} from '../../api/presets/delete'
import {exportPresets} from '../../api/presets/export'
import {importPresets} from '../../api/presets/import'
import {listPresets} from '../../api/presets/list'
import {getPresetTemplates} from '../../api/presets/templates'
import type {Preset, PresetIn, PresetsFile} from '../../api/presets/types'
import {updatePreset} from '../../api/presets/update'
import type {ManagedModel} from '../../api/runtime/types'
import {errorReason} from '../../utils/error-reason.ts'
import {ConfirmPopup} from '../popups/base/confirm-popup.tsx'
import {PresetEditorPopup} from '../popups/preset-editor-popup.tsx'
import {Button, Div, Label, Select} from '../primitives'

export interface PresetsPanelProps {
    models: ManagedModel[]
    /** The model that is loaded right now, which the model selector starts on */
    loadedModelId: number | null
}

const blank = (modelId: number | null): PresetIn => ({
    model_id: modelId, name: '', temperature: null, top_p: null, top_k: null, min_p: null,
    repeat_penalty: null, presence_penalty: null, seed: null,
})

const summary = (p: Preset): string =>
    [
        p.temperature != null ? `temp ${p.temperature}` : null,
        p.top_p != null ? `top-p ${p.top_p}` : null,
        p.top_k != null ? `top-k ${p.top_k}` : null,
        p.min_p != null ? `min-p ${p.min_p}` : null,
        p.repeat_penalty != null ? `repeat ${p.repeat_penalty}` : null,
        p.presence_penalty != null ? `presence ${p.presence_penalty}` : null,
        p.seed != null ? `seed ${p.seed}` : null,
    ].filter(Boolean).join(' · ') || 'the server\'s defaults'

/** The starting point that fills nothing in */
const EMPTY = 'Empty'

/** A preset's name, with what it applies to when that isn't just the model on screen */
const presetTitle = (p: Preset): string =>
    p.removed_model ? `${p.name} (deleted model: ${p.removed_model})` : p.model_id == null ? `${p.name} (any model)` : p.name

const label = (m: ManagedModel): string => m.display_name ?? m.file

/**
 * The user's own sampling presets for a model: named sets of temperature and friends, one chosen at
 * a time per model. Built-in templates are starting points to copy, and the whole set can be
 * exported to a file and imported again (on another install, say).
 */
export const PresetsPanel = ({models, loadedModelId}: PresetsPanelProps) => {
    // What the user picked; until then the loaded model, else the first one
    const [pickedModelId, setModelId] = useState<number | null>(null)
    const modelId = pickedModelId ?? loadedModelId ?? models[0]?.id ?? null
    const [presets, setPresets] = useState<Preset[]>([])
    const [chosen, setChosen] = useState<number | null>(null)
    const [templates, setTemplates] = useState<PresetIn[]>([])
    const [template, setTemplate] = useState<string>(EMPTY)
    const [editing, setEditing] = useState<{ id: number | null; initial: PresetIn } | null>(null)
    const [removing, setRemoving] = useState<Preset | null>(null)
    const [note, setNote] = useState<{ text: string; bad?: boolean } | null>(null)
    const fileInput = useRef<HTMLInputElement>(null)

    const reload = useCallback(async () => {
        if (modelId == null) return
        try {
            const result = await listPresets(modelId)
            setPresets(result.presets)
            setChosen(result.chosen)
        } catch {
            setNote({text: 'Could not load the presets.', bad: true})
        }
    }, [modelId])

    useEffect(() => {
        void reload()
    }, [reload])

    useEffect(() => {
        getPresetTemplates().then(setTemplates).catch(() => setTemplates([]))
    }, [])

    if (models.length === 0) {
        return <Label variant="secondary" text="Add a model first — presets belong to a model."/>
    }

    // A preset is for this model or for any model; the other models' presets aren't shown
    const shown = presets.filter((p) => p.model_id === modelId || p.model_id == null)

    const run = async (action: () => Promise<void>, failure: string) => {
        setNote(null)
        try {
            await action()
        } catch (e) {
            setNote({text: errorReason(e, failure), bad: true})
        }
        await reload()
    }

    const save = async (preset: PresetIn) => {
        if (editing?.id != null) await updatePreset(editing.id, preset)
        else await createPreset(preset)
        await reload()
    }

    const onExport = () => run(async () => {
        const file = await exportPresets()
        const url = URL.createObjectURL(new Blob([JSON.stringify(file, null, 2)], {type: 'application/json'}))
        const link = document.createElement('a')
        link.href = url
        link.download = 'sampling-presets.json'
        link.click()
        URL.revokeObjectURL(url)
    }, 'Could not export the presets.')

    const onImportFile = (file: File | undefined) => {
        if (!file) return
        void run(async () => {
            let parsed: PresetsFile
            try {
                parsed = JSON.parse(await file.text()) as PresetsFile
            } catch {
                throw new Error('that file is not JSON')
            }
            const result = await importPresets(parsed)
            setNote({
                text: `Imported ${result.imported}` + (result.renamed ? `, ${result.renamed} renamed` : '') +
                    (result.any_model ? `, ${result.any_model} for a model not on this install (now for any model)` : '') + '.',
            })
        }, 'Could not import that file.')
        if (fileInput.current) fileInput.current.value = ''
    }

    // New preset starts from the chosen template's values (Empty fills nothing in)
    const startingPoint = (): PresetIn => {
        const chosenTemplate = templates.find((t) => t.name === template)
        return chosenTemplate ? {...chosenTemplate, name: '', model_id: modelId} : blank(modelId)
    }

    return (
        <Div className="models__section">
            <Div className="models__fields">
                <Label className="field__label" text="Model"/>
                <Select values={models.map(label)} selected={label(models.find((m) => m.id === modelId) ?? models[0])}
                        onChosen={(value) => setModelId(models.find((m) => label(m) === value)?.id ?? null)}/>
            </Div>

            {note ? <Label variant="secondary" className={note.bad ? 'models__error' : 'models__meta'} text={note.text}/> : null}

            <Div className={`models__preset${chosen == null ? ' models__profile--active' : ''}`}>
                <Div className="models__profile-main">
                    <Label className="models__profile-name" text="Server defaults"/>
                    <Label variant="secondary" className="models__meta" text="No sampling values are sent; the model's own recommended ones apply."/>
                </Div>
                {chosen == null ? <Label variant="secondary" className="models__meta" text="in use"/> :
                    <Button variant="secondary" text="Use" onClicked={() => modelId != null && void run(() => choosePreset(modelId, null), 'Could not choose it.')}/>}
            </Div>

            {shown.map((preset) => (
                <Div key={preset.id} className={`models__preset${chosen === preset.id ? ' models__profile--active' : ''}`}>
                    <Div className="models__profile-main">
                        <Label className="models__profile-name" text={presetTitle(preset)}/>
                        <Label variant="secondary" className="models__meta" text={summary(preset)}/>
                    </Div>
                    <Div className="models__profile-actions">
                        {chosen === preset.id ? <Label variant="secondary" className="models__meta" text="in use"/> :
                            <Button variant="secondary" text="Use"
                                    onClicked={() => modelId != null && void run(() => choosePreset(modelId, preset.id), 'Could not choose it.')}/>}
                        <Button variant="secondary" text="Edit" onClicked={() => setEditing({id: preset.id, initial: preset})}/>
                        <Button variant="secondary" text="Delete" onClicked={() => setRemoving(preset)}/>
                    </Div>
                </Div>
            ))}

            <Div className="models__toolbar">
                <Button text="New preset" onClicked={() => setEditing({id: null, initial: startingPoint()})}/>
                {templates.length > 0 ? (
                    <>
                        <Label variant="secondary" className="models__meta" text="starting from"/>
                        <Select values={[EMPTY, ...templates.map((t) => t.name)]} selected={template} onChosen={setTemplate}/>
                    </>
                ) : null}
                <Button variant="secondary" text="Export" onClicked={() => void onExport()}/>
                <Button variant="secondary" text="Import" onClicked={() => fileInput.current?.click()}/>
                <input ref={fileInput} type="file" accept="application/json,.json" hidden
                       onChange={(e) => onImportFile(e.target.files?.[0])}/>
            </Div>

            <PresetEditorPopup
                open={editing != null}
                title={editing?.id != null ? 'Edit preset' : 'New preset'}
                initial={editing?.initial ?? blank(modelId)}
                onSave={save}
                onClose={() => setEditing(null)}
            />
            <ConfirmPopup
                open={removing != null}
                title="Delete preset"
                message={`Delete "${removing?.name}"?`}
                confirmLabel="Delete"
                onConfirm={() => removing && void run(() => deletePreset(removing.id), 'Could not delete it.')}
                onClose={() => setRemoving(null)}
            />
        </Div>
    )
};
