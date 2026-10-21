import {useState, type CSSProperties} from 'react'
import '../styles/model-picker.scss'
import '../styles/voice.scss'
import {installSttModel} from '../api/stt/install.ts'
import {removeSttModel} from '../api/stt/remove.ts'
import type {SttModel, SttStatus, SttTask} from '../api/stt/types.ts'
import {errorReason} from '../utils/error-reason.ts'
import {formatBytes} from '../utils/format-bytes.ts'
import {Button, Div, Label} from './primitives'
import {SettingsRow} from './settings-fields.tsx'

export interface VoiceModelsProps {
    status: SttStatus
    /** The owner installs and removes; anyone else sees what there is */
    canManage: boolean
    /** Called after an install was started or a model removed: the status is read again */
    onChanged: () => void
}

/** Where an install is, in words: a download with how much of it is done, then the checking and the unpacking */
const progressText = (task: SttTask): string => {
    switch (task.phase) {
        case 'downloading':
            return `Downloading ${formatBytes(task.completed_bytes)} of ${formatBytes(task.total_bytes)}`
        case 'checking':
            return 'Checking the download'
        case 'unpacking':
            return `Unpacking the files · ${Math.round(fractionOf(task) * 100)}%`
        default:
            return 'Starting'
    }
}

/** How much of the current phase is done, 0 to 1 (the checking has no steps, and is short: the bar stays full) */
const fractionOf = (task: SttTask): number =>
    task.phase === 'checking' ? 1 : task.total_bytes > 0 ? Math.min(1, task.completed_bytes / task.total_bytes) : 0

/** What a model is and what it takes, so the owner can choose before downloading */
const detailsOf = (model: SttModel): string =>
    `${model.description} Languages: ${model.languages}. Download ${formatBytes(model.download_bytes)}, about ${formatBytes(model.memory_bytes)} of memory while it is loaded; it runs on the CPU.`

/** The speech models, each with its install, removal and progress; shared by Settings → Voice and the setup wizard */
export const VoiceModels = ({status, canManage, onChanged}: VoiceModelsProps) => {
    const [error, setError] = useState<string | null>(null)
    const installing = status.task?.state === 'running' ? status.task : null

    const install = async (model: SttModel) => {
        setError(null)
        try {
            await installSttModel(model.id)
        } catch (e) {
            setError(errorReason(e, 'The install could not be started.'))
        }
        onChanged()
    }
    const remove = async (model: SttModel) => {
        setError(null)
        try {
            await removeSttModel(model.id)
        } catch (e) {
            setError(errorReason(e, 'The model could not be removed.'))
        }
        onChanged()
    }

    return (
        <>
            {status.models.map((model) => {
                const mine = installing?.model === model.id ? installing : null
                const failed = status.task?.state === 'failed' && status.task.model === model.id && !model.installed ? status.task.error : null
                const below = mine || failed ? (
                    <>
                        {mine ? (
                            <>
                                <Label className="voice-model__state" text={progressText(mine)}/>
                                <Div className="model-picker__bar">
                                    <Div className="model-picker__bar-fill" style={{'--fraction': fractionOf(mine)} as CSSProperties}/>
                                </Div>
                                {mine.note ? <Label className="voice-model__note" text={mine.note}/> : null}
                            </>
                        ) : null}
                        {failed ? <Label className="voice-model__error" text={`The install failed: ${failed}${status.task?.completed_bytes ? ' What was downloaded is kept, and Try again goes on from it.' : ''}`}/> : null}
                    </>
                ) : null
                return (
                    <SettingsRow key={model.id} label={model.label} top below={below}
                                 help={model.installed ? `Installed, ${formatBytes(model.disk_bytes ?? 0)} on disk. ${detailsOf(model)}` : detailsOf(model)}>
                        {mine ? (
                            <Label className="voice-model__state" text="Installing…"/>
                        ) : model.installed ? (
                            canManage ? <Button variant="secondary" text="Remove" onClicked={() => void remove(model)}/> : null
                        ) : canManage ? (
                            <Button variant="secondary" text={failed ? 'Try again' : 'Install'} disabled={installing != null} onClicked={() => void install(model)}/>
                        ) : (
                            <Label className="voice-model__state" text="Not installed"/>
                        )}
                    </SettingsRow>
                )
            })}
            {error ? <Label className="voice-model__error" text={error}/> : null}
            {!canManage ? <Label variant="secondary" text="Only the owner of this server can install or remove models."/> : null}
        </>
    )
};
