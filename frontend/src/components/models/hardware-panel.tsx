import {useState} from 'react'
import {getDevices} from '../../api/runtime/devices'
import type {RuntimeStatus} from '../../api/runtime/types'
import {errorReason} from '../../utils/error-reason.ts'
import {Button, Div, Label} from '../primitives'

export interface HardwarePanelProps {
    status: RuntimeStatus | null
}

/** What llama.cpp itself sees: its own device list (the authority on whether the GPU is usable),
 * and where the loaded model went */
export const HardwarePanel = ({status}: HardwarePanelProps) => {
    const [output, setOutput] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)

    const check = async () => {
        setBusy(true)
        setError(null)
        try {
            setOutput(await getDevices())
        } catch (e) {
            setError(errorReason(e, 'Could not ask llama.cpp for its devices.'))
        } finally {
            setBusy(false)
        }
    }

    const facts = status?.facts
    return (
        <Div className="models__section">
            <Label variant="secondary" className="models__meta"
                   text="The devices below are what llama-server reports for itself; if the GPU is missing here, the model will run on the CPU."/>
            <Div className="models__toolbar">
                <Button text={busy ? 'Asking…' : 'Check devices'} disabled={busy} onClicked={() => void check()}/>
            </Div>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            {output != null ? <pre className="models__pre">{output.trim() || 'llama-server printed nothing.'}</pre> : null}

            {facts && status?.state === 'ready' ? (
                <>
                    <Label variant="secondary" className="models__heading" text="Loaded model"/>
                    {facts.layers_offloaded != null ? (
                        <Label className="models__meta" text={`${facts.layers_offloaded} of ${facts.layers_total} layers on the GPU`}/>
                    ) : null}
                    {facts.buffers.map((b) => (
                        <Label key={b.name} variant="secondary" className="models__meta" text={`${b.name}: ${Math.round(b.mib)} MiB`}/>
                    ))}
                </>
            ) : null}
        </Div>
    )
};
