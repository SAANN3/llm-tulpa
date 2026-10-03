import {useState} from 'react'
import {loadProfile} from '../../api/runtime/load'
import {stopRuntime} from '../../api/runtime/stop'
import type {RuntimeStatus} from '../../api/runtime/types'
import {errorReason} from '../../utils/error-reason.ts'
import {useUptime} from '../../hooks/use-uptime.ts'
import {formatDurationShort} from '../../utils/format.ts'
import {RuntimeLogsPopup} from '../popups/runtime-logs-popup.tsx'
import {Button, Div, Label} from '../primitives'

export interface RuntimePanelProps {
    status: RuntimeStatus | null
    /** The loaded profile's name, when it is known */
    profileName: string | null
    isOwner: boolean
    /** The profile Load starts: the first of the user's default model; null when there is none */
    defaultProfileId: number | null
    onChanged: () => void
}

const HEADLINE: Record<RuntimeStatus['state'], string> = {
    stopped: 'Stopped — it loads when a chat needs it',
    starting: 'Loading the model — this can take a minute, keep the backend running',
    ready: 'Ready',
    failed: 'Failed to load',
    not_installed: 'llama.cpp is not installed',
    external: 'Using an external llama-server',
}

/** What the model server is doing right now, with where the model went and who is using it */
export const RuntimePanel = ({status, profileName, isOwner, defaultProfileId, onChanged}: RuntimePanelProps) => {
    const [logsOpen, setLogsOpen] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const [loading, setLoading] = useState(false)
    const uptime = useUptime(status?.uptime_secs ?? null)

    if (!status) return <Label variant="secondary" text="Asking the model server…"/>

    const onStop = async () => {
        setError(null)
        try {
            await stopRuntime()
        } catch (e) {
            setError(errorReason(e, 'Could not stop the model server.'))
        }
        onChanged()
    }

    // Returns once the model is ready, which can take a minute; the state events refresh the panel meanwhile
    const onLoad = async () => {
        if (defaultProfileId == null) return
        setError(null)
        setLoading(true)
        try {
            await loadProfile(defaultProfileId)
        } catch (e) {
            setError(errorReason(e, 'Could not load the model.'))
        } finally {
            setLoading(false)
            onChanged()
        }
    }

    const {facts} = status
    const canLoad = status.state === 'stopped' || status.state === 'failed'
    const placed =
        facts.layers_offloaded != null && facts.layers_total != null
            ? `${facts.layers_offloaded}/${facts.layers_total} layers on the GPU`
            : null

    return (
        <Div className="models__runtime">
            <Div className="models__runtime-head">
                <Label className={`models__state models__state--${status.state}`} text={HEADLINE[status.state]}/>
                <Div className="models__runtime-actions">
                    {canLoad ? (
                        <Button variant="secondary" text={loading ? 'Loading…' : 'Load'} disabled={loading || defaultProfileId == null}
                                onClicked={() => void onLoad()}/>
                    ) : null}
                    {isOwner && status.state === 'ready' ? <Button variant="secondary" text="Stop" onClicked={() => void onStop()}/> : null}
                    {isOwner ? <Button variant="secondary" text="Log" onClicked={() => setLogsOpen(true)}/> : null}
                </Div>
            </Div>

            {status.detail && status.state !== 'external' ? (
                <Label variant="secondary" className="models__error" text={status.detail}/>
            ) : null}
            {/* A failed load already shows its reason above; this is for what the state doesn't say (the model is in use, say) */}
            {error && status.state !== 'failed' ? <Label variant="secondary" className="models__error" text={error}/> : null}

            {status.model ? (
                <Label variant="secondary" className="models__meta"
                       text={[profileName ? `${status.model} · ${profileName}` : status.model,
                           uptime != null ? `up ${formatDurationShort(uptime)}` : '']
                           .filter(Boolean).join(' · ')}/>
            ) : null}
            {status.state === 'ready' ? (
                <Label variant="secondary" className="models__meta"
                       text={[placed, facts.context_per_slot ? `context ${facts.context_per_slot}` : '',
                           ...facts.buffers.map((b) => `${b.name} ${Math.round(b.mib)} MiB`)].filter(Boolean).join(' · ')}/>
            ) : null}
            {status.state === 'ready' && status.placement ? (
                <Label variant="secondary" className={status.placement.verdict === 'gpu' || status.placement.verdict === 'unknown' ? 'models__meta' : 'models__error'}
                       text={status.placement.summary}/>
            ) : null}
            {status.holders.length > 0 ? (
                <Label variant="secondary" className="models__meta" text={`In use by ${status.holders.join(', ')}`}/>
            ) : null}

            <RuntimeLogsPopup open={logsOpen} onClose={() => setLogsOpen(false)}/>
        </Div>
    )
};
