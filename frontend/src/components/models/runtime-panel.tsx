import {useState} from 'react'
import {stopRuntime} from '../../api/runtime/stop'
import type {RuntimeStatus} from '../../api/runtime/types'
import {errorReason} from '../../utils/error-reason.ts'
import {formatDurationShort} from '../../utils/format.ts'
import {RuntimeLogsPopup} from '../popups/runtime-logs-popup.tsx'
import {Button, Div, Label} from '../primitives'

export interface RuntimePanelProps {
    status: RuntimeStatus | null
    /** The loaded profile's name, when it is known */
    profileName: string | null
    isOwner: boolean
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
export const RuntimePanel = ({status, profileName, isOwner, onChanged}: RuntimePanelProps) => {
    const [logsOpen, setLogsOpen] = useState(false)
    const [error, setError] = useState<string | null>(null)

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

    const {facts} = status
    const placed =
        facts.layers_offloaded != null && facts.layers_total != null
            ? `${facts.layers_offloaded}/${facts.layers_total} layers on the GPU`
            : null

    return (
        <Div className="models__runtime">
            <Div className="models__runtime-head">
                <Label className={`models__state models__state--${status.state}`} text={HEADLINE[status.state]}/>
                <Div className="models__runtime-actions">
                    {isOwner && status.state === 'ready' ? <Button variant="secondary" text="Stop" onClicked={() => void onStop()}/> : null}
                    {isOwner ? <Button variant="secondary" text="Log" onClicked={() => setLogsOpen(true)}/> : null}
                </Div>
            </Div>

            {status.detail && status.state !== 'external' ? (
                <Label variant="secondary" className="models__error" text={status.detail}/>
            ) : null}
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}

            {status.model ? (
                <Label variant="secondary" className="models__meta"
                       text={[profileName ? `${status.model} · ${profileName}` : status.model,
                           status.uptime_secs != null ? `up ${formatDurationShort(status.uptime_secs)}` : '']
                           .filter(Boolean).join(' · ')}/>
            ) : null}
            {status.state === 'ready' ? (
                <Label variant="secondary" className="models__meta"
                       text={[placed, facts.context_per_slot ? `context ${facts.context_per_slot}` : '',
                           ...facts.buffers.map((b) => `${b.name} ${Math.round(b.mib)} MiB`)].filter(Boolean).join(' · ')}/>
            ) : null}
            {status.holders.length > 0 ? (
                <Label variant="secondary" className="models__meta" text={`In use by ${status.holders.join(', ')}`}/>
            ) : null}

            <RuntimeLogsPopup open={logsOpen} onClose={() => setLogsOpen(false)}/>
        </Div>
    )
};
