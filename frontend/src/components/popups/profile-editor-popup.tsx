import {useState, type ReactNode} from 'react'
import type {LaunchProfile, LaunchProfileIn} from '../../api/profiles/types'
import {errorReason} from '../../utils/error-reason.ts'
import {numberText, parseNumber} from '../../utils/parse-number.ts'
import {Div, Input, Label, Select, ToggleSwitch} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

/** The KV cache types llama-server accepts, as the backend validates them */
const CACHE_TYPES = ['f32', 'f16', 'bf16', 'q8_0', 'q4_0', 'q4_1', 'iq4_nl', 'q5_0', 'q5_1']
const NO_PROJECTOR = 'no vision'

export interface ProfileEditorPopupProps {
    open: boolean
    /** The model file the profile launches, shown so it is clear what is being edited */
    modelFile: string
    /** The profile being changed; null makes a new one from the defaults */
    profile: LaunchProfile | null
    /** Projector files in the model folder that fit this model */
    projectors: string[]
    /** What the model's file says about itself, to explain the fields */
    hasMtp: boolean
    layers: number | null
    trainedContext: number | null
    /** Rejects with the backend's reason when the profile is refused */
    onSave: (settings: LaunchProfileIn) => Promise<void>
    onClose: () => void
}

const Field = ({label, hint, children}: { label: string; hint?: string; children: ReactNode }) => (
    <Div className="field">
        <Label className="field__label" text={label}/>
        {children}
        {hint ? <Label variant="secondary" className="field__help" text={hint}/> : null}
    </Div>
)

// Its own component so the draft lives only while the popup is open, like `InputPopup`.
const ProfileForm = ({modelFile, profile, projectors, hasMtp, layers, trainedContext, onSave, onClose}: Omit<ProfileEditorPopupProps, 'open'>) => {
    const [name, setName] = useState(profile?.name ?? '')
    const [context, setContext] = useState(numberText(profile?.context_length ?? null))
    const [cacheK, setCacheK] = useState(profile?.cache_type_k ?? 'q8_0')
    const [cacheV, setCacheV] = useState(profile?.cache_type_v ?? 'q8_0')
    const [flashAttn, setFlashAttn] = useState(profile?.flash_attn ?? true)
    const [gpuLayers, setGpuLayers] = useState(String(profile?.gpu_layers ?? 99))
    const [mtp, setMtp] = useState(profile?.mtp ?? hasMtp)
    const [draft, setDraft] = useState(String(profile?.spec_draft_n_max ?? 3))
    const [ngramMatch, setNgramMatch] = useState(String(profile?.ngram_match ?? 24))
    const [ngramMin, setNgramMin] = useState(String(profile?.ngram_min ?? 8))
    const [ngramMax, setNgramMax] = useState(String(profile?.ngram_max ?? 32))
    const [projector, setProjector] = useState(profile?.mmproj_file ?? NO_PROJECTOR)
    const [projectorGpu, setProjectorGpu] = useState(profile?.mmproj_gpu ?? true)
    const [extra, setExtra] = useState(profile?.extra_args ?? '')
    const [error, setError] = useState<string | null>(null)
    const [saving, setSaving] = useState(false)

    const save = async () => {
        setSaving(true)
        setError(null)
        try {
            await onSave({
                name: name.trim(),
                context_length: parseNumber(context),
                cache_type_k: cacheK,
                cache_type_v: cacheV,
                flash_attn: flashAttn,
                gpu_layers: parseNumber(gpuLayers) ?? 99,
                mtp,
                spec_draft_n_max: parseNumber(draft) ?? 3,
                ngram_match: parseNumber(ngramMatch) ?? 24,
                ngram_min: parseNumber(ngramMin) ?? 8,
                ngram_max: parseNumber(ngramMax) ?? 32,
                mmproj_file: projector === NO_PROJECTOR ? null : projector,
                mmproj_gpu: projectorGpu,
                extra_args: extra.trim(),
            })
            onClose()
        } catch (e) {
            setError(errorReason(e, 'Could not save the profile.'))
        } finally {
            setSaving(false)
        }
    }

    return (
        <>
            <Div className="field">
                <Label className="field__label" text="Model"/>
                <Label className="models__meta" text={modelFile}/>
            </Div>
            <Field label="Name"><Input text={name} onChanged={setName} placeholder="e.g. Long context"/></Field>
            <Field label="Context (tokens)"
                   hint={`Empty sizes it to free memory.${trainedContext ? ` The model was trained for ${trainedContext}.` : ''}`}>
                <Input text={context} onChanged={setContext} placeholder="auto"/>
            </Field>
            <Div className="models__fields">
                <Div className="field">
                    <Label className="field__label" text="KV cache K"/>
                    <Select values={CACHE_TYPES} selected={cacheK} onChosen={setCacheK}/>
                </Div>
                <Div className="field">
                    <Label className="field__label" text="KV cache V"/>
                    <Select values={CACHE_TYPES} selected={cacheV} onChosen={setCacheV}/>
                </Div>
            </Div>
            <Div className="field__row">
                <Label className="field__row-label" text="Flash attention"/>
                <ToggleSwitch toggled={flashAttn} onToggled={setFlashAttn}/>
            </Div>
            <Field label="Layers on the GPU"
                   hint={layers ? `The model has ${layers} layers; 99 puts everything on the GPU.` : '99 puts everything on the GPU.'}>
                <Input text={gpuLayers} onChanged={setGpuLayers}/>
            </Field>
            <Div className="field__row">
                <Label className="field__row-label" text="MTP drafting"/>
                <ToggleSwitch toggled={mtp} onToggled={setMtp} disabled={!hasMtp && !profile?.mtp}/>
            </Div>
            {!hasMtp ? <Label variant="secondary" className="field__help" text="This file has no MTP draft head, so the switch has no effect."/> : null}
            {mtp ? (
                <Div className="models__fields">
                    <Div className="field"><Label className="field__label" text="Draft"/><Input text={draft} onChanged={setDraft}/></Div>
                    <Div className="field"><Label className="field__label" text="n-gram match"/><Input text={ngramMatch} onChanged={setNgramMatch}/></Div>
                    <Div className="field"><Label className="field__label" text="min"/><Input text={ngramMin} onChanged={setNgramMin}/></Div>
                    <Div className="field"><Label className="field__label" text="max"/><Input text={ngramMax} onChanged={setNgramMax}/></Div>
                </Div>
            ) : null}
            <Field label="Vision projector">
                <Select values={[NO_PROJECTOR, ...projectors]} selected={projector} onChosen={setProjector}/>
            </Field>
            {projector !== NO_PROJECTOR ? (
                <>
                    <Div className="field__row">
                        <Label className="field__row-label" text="Projector on the GPU"/>
                        <ToggleSwitch toggled={projectorGpu} onToggled={setProjectorGpu}/>
                    </Div>
                    <Label variant="secondary" className="field__help"
                           text="Off keeps the vision projector in RAM: it takes no VRAM, and reading an image takes a few seconds longer. Replies are not slower."/>
                </>
            ) : null}
            <Field label="Extra arguments" hint="Passed to llama-server as they are, split on spaces (no shell).">
                <Input text={extra} onChanged={setExtra} placeholder="--no-mmap"/>
            </Field>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            <PopupActions confirmLabel={saving ? 'Saving…' : 'Save'} confirmDisabled={saving || !name.trim()}
                          onConfirm={() => void save()} onCancel={onClose} emphasis="confirm"/>
        </>
    )
}

/** Edits how one model is launched: context, KV cache, GPU layers, MTP, vision projector */
export const ProfileEditorPopup = ({open, onClose, ...rest}: ProfileEditorPopupProps) => (
    <Popup open={open} onClose={onClose} title={rest.profile ? 'Edit launch profile' : 'New launch profile'} width={520}>
        <ProfileForm {...rest} onClose={onClose}/>
    </Popup>
);
