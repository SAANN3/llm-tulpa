import {useState} from 'react'
import type {LocalFile, LocalFiles} from '../../api/llm/types'
import {createProfile} from '../../api/profiles/create'
import {deleteProfile} from '../../api/profiles/delete'
import type {LaunchProfile, LaunchProfileIn} from '../../api/profiles/types'
import {updateProfile} from '../../api/profiles/update'
import {loadProfile} from '../../api/runtime/load'
import {registerModel} from '../../api/runtime/register'
import {runSpeedTest} from '../../api/runtime/test'
import type {ManagedModel, RuntimeStatus, SpeedTest} from '../../api/runtime/types'
import {errorReason} from '../../utils/error-reason.ts'
import {formatBytes} from '../../utils/format-bytes.ts'
import {ConfirmPopup} from '../popups/base/confirm-popup.tsx'
import {ProfileEditorPopup} from '../popups/profile-editor-popup.tsx'
import {Button, Div, Label, Select} from '../primitives'

export interface ModelsPanelProps {
    status: RuntimeStatus | null
    models: ManagedModel[]
    profiles: LaunchProfile[]
    /** The `.gguf` files on disk (owner only); null for anyone else */
    files: LocalFiles | null
    isOwner: boolean
    /** Reloads the models and the server's state after a change */
    onChanged: () => void
}

const NO_PROJECTOR = 'no vision'

/** What a profile does, in a line */
const summary = (p: LaunchProfile): string =>
    [
        p.context_length ? `${p.context_length} ctx` : 'auto ctx',
        `KV ${p.cache_type_k}/${p.cache_type_v}`,
        p.mtp ? 'MTP' : null,
        p.mmproj_file ? 'vision' : null,
        p.gpu_layers < 99 ? `${p.gpu_layers} GPU layers` : null,
    ].filter(Boolean).join(' · ')

const speedLine = (t: SpeedTest): string =>
    [
        t.generated_tokens_per_second != null ? `${t.generated_tokens_per_second.toFixed(1)} tok/s writing` : null,
        t.prompt_tokens_per_second != null ? `${t.prompt_tokens_per_second.toFixed(0)} tok/s reading` : null,
        t.draft_acceptance != null ? `${Math.round(t.draft_acceptance * 100)}% of MTP drafts accepted` : null,
    ].filter(Boolean).join(' · ') || 'no timing reported'

/**
 * The models the backend runs itself, each with its launch profiles. Anyone can load a profile or
 * test its speed; the owner can also edit, add and remove profiles and register more model files.
 */
export const ModelsPanel = ({status, models, profiles, files, isOwner, onChanged}: ModelsPanelProps) => {
    const [busy, setBusy] = useState<number | null>(null)
    const [message, setMessage] = useState<{ profileId: number; text: string; bad?: boolean } | null>(null)
    const [editing, setEditing] = useState<{ model: ManagedModel; profile: LaunchProfile | null } | null>(null)
    const [removing, setRemoving] = useState<LaunchProfile | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [projectors, setProjectors] = useState<Record<string, string>>({})

    const fileOf = (model: ManagedModel): LocalFile | undefined => files?.files.find((f) => f.path === model.file)

    const onLoad = async (profile: LaunchProfile) => {
        setBusy(profile.id)
        setMessage(null)
        try {
            await loadProfile(profile.id)
            setMessage({profileId: profile.id, text: 'Loaded.'})
        } catch (e) {
            setMessage({profileId: profile.id, text: errorReason(e, 'Could not load it.'), bad: true})
        } finally {
            setBusy(null)
            onChanged()
        }
    }

    const onTest = async (profile: LaunchProfile) => {
        setBusy(profile.id)
        setMessage(null)
        try {
            setMessage({profileId: profile.id, text: speedLine(await runSpeedTest(profile.id))})
        } catch (e) {
            setMessage({profileId: profile.id, text: errorReason(e, 'The test failed.'), bad: true})
        } finally {
            setBusy(null)
            onChanged()
        }
    }

    const onDelete = async (profile: LaunchProfile) => {
        setError(null)
        try {
            await deleteProfile(profile.id)
        } catch (e) {
            setError(errorReason(e, 'Could not delete the profile.'))
        }
        onChanged()
    }

    const onRegister = async (file: LocalFile) => {
        setError(null)
        const projector = projectors[file.path] ?? file.suggested_projector ?? NO_PROJECTOR
        try {
            await registerModel(file.path, undefined, projector === NO_PROJECTOR ? undefined : projector)
        } catch (e) {
            setError(errorReason(e, 'Could not add the model.'))
        }
        onChanged()
    }

    const save = async (settings: LaunchProfileIn) => {
        if (!editing) return
        if (editing.profile) await updateProfile(editing.profile.id, settings)
        else await createProfile(editing.model.id, settings)
        onChanged()
    }

    const registered = new Set(models.map((m) => m.file))
    const addable = (files?.files ?? []).filter((f) => f.kind === 'model' && !registered.has(f.path))
    const editedFile = editing ? fileOf(editing.model) : undefined

    return (
        <Div className="models__section">
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}

            {models.length === 0 ? (
                <Label variant="secondary"
                       text={isOwner ? 'No model added yet — add one of the files below.' : 'No model has been added yet; ask the owner.'}/>
            ) : null}

            {models.map((model) => {
                const file = fileOf(model)
                const own = profiles.filter((p) => model.profile_ids.includes(p.id))
                return (
                    <Div key={model.id} className="models__model">
                        <Div className="models__model-head">
                            <Label className="models__model-name" text={model.display_name ?? model.file}/>
                            {isOwner ? <Button variant="secondary" text="New profile"
                                               onClicked={() => setEditing({model, profile: null})}/> : null}
                        </Div>
                        <Label variant="secondary" className="models__meta"
                               text={[model.display_name ? model.file : null, file?.quantization, file ? formatBytes(file.size_bytes) : null,
                                   file?.has_mtp ? 'has MTP head' : null].filter(Boolean).join(' · ')}/>
                        {own.map((profile) => {
                            const active = status?.profile_id === profile.id && status.state === 'ready'
                            return (
                                <Div key={profile.id} className={`models__profile${active ? ' models__profile--active' : ''}`}>
                                    <Div className="models__profile-main">
                                        <Label className="models__profile-name" text={profile.name}/>
                                        <Label variant="secondary" className="models__meta" text={summary(profile)}/>
                                        {message?.profileId === profile.id ? (
                                            <Label variant="secondary" className={message.bad ? 'models__error' : 'models__meta'}
                                                   text={message.text}/>
                                        ) : null}
                                    </Div>
                                    <Div className="models__profile-actions">
                                        {active ? <Label variant="secondary" className="models__meta" text="loaded"/> :
                                            <Button variant="secondary" text="Load" disabled={busy != null}
                                                    onClicked={() => void onLoad(profile)}/>}
                                        <Button variant="secondary" text={busy === profile.id ? 'Running…' : 'Test'}
                                                disabled={busy != null} onClicked={() => void onTest(profile)}/>
                                        {isOwner ? <Button variant="secondary" text="Edit" onClicked={() => setEditing({model, profile})}/> : null}
                                        {isOwner && own.length > 1 ? <Button variant="secondary" text="Delete" onClicked={() => setRemoving(profile)}/> : null}
                                    </Div>
                                </Div>
                            )
                        })}
                    </Div>
                )
            })}

            {isOwner ? (
                <>
                    <Label variant="secondary" className="models__heading" text="Model files"/>
                    {files && !files.configured ? (
                        <Label variant="secondary" className="models__meta"
                               text="No model folder is configured — set model_dir in the backend's settings.json."/>
                    ) : addable.length === 0 ? (
                        <Label variant="secondary" className="models__meta" text="Every .gguf file in the model folder is added."/>
                    ) : null}
                    {addable.map((file) => (
                        <Div key={file.path} className="models__file">
                            <Div className="models__profile-main">
                                <Label className="models__profile-name" text={file.path}/>
                                <Label variant="secondary" className="models__meta"
                                       text={[file.quantization, formatBytes(file.size_bytes), file.has_mtp ? 'has MTP head' : null].filter(Boolean).join(' · ')}/>
                            </Div>
                            {file.compatible_projectors.length > 0 ? (
                                <Select values={[NO_PROJECTOR, ...file.compatible_projectors]}
                                        selected={projectors[file.path] ?? file.suggested_projector ?? NO_PROJECTOR}
                                        onChosen={(value) => setProjectors((prev) => ({...prev, [file.path]: value}))}/>
                            ) : null}
                            <Button variant="secondary" text="Add" onClicked={() => void onRegister(file)}/>
                        </Div>
                    ))}
                </>
            ) : null}

            <ProfileEditorPopup
                open={editing != null}
                profile={editing?.profile ?? null}
                projectors={(files?.files ?? []).filter((f) => f.kind === 'projector').map((f) => f.path)}
                hasMtp={editedFile?.has_mtp ?? false}
                layers={editedFile?.block_count ?? null}
                trainedContext={editedFile?.trained_context ?? null}
                onSave={save}
                onClose={() => setEditing(null)}
            />
            <ConfirmPopup
                open={removing != null}
                title="Delete launch profile"
                message={`Delete "${removing?.name}"? Chats using it move to the model's first profile.`}
                confirmLabel="Delete"
                onConfirm={() => removing && void onDelete(removing)}
                onClose={() => setRemoving(null)}
            />
        </Div>
    )
};
