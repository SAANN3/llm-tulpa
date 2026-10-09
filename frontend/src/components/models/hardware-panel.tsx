import {useEffect, useState} from 'react'
import {getDevices} from '../../api/runtime/devices'
import {getModelFolder, type ModelFolder} from '../../api/runtime/folder'
import {getSystemSnapshot, type SystemSnapshot} from '../../api/runtime/system'
import type {RuntimeStatus} from '../../api/runtime/types'
import {errorReason} from '../../utils/error-reason.ts'
import {formatBytes} from '../../utils/format-bytes.ts'
import {Button, Div, Label} from '../primitives'
import {SettingsRow} from '../settings-fields.tsx'

export interface HardwarePanelProps {
    status: RuntimeStatus | null
}

/** How often the figures are read again while the tab is open */
const POLL_MS = 2000

const MIB = 1024 * 1024

interface BarProps {
    label: string
    /** 0 to 1 of the whole */
    used: number
    /** 0 to 1 of the whole held by the model server, drawn brighter at the start of the bar */
    held?: number
    text: string
}

/** One figure as a bar, like a task manager: the model server's share bright, the rest dim */
const LoadBar = ({label, used, held, text}: BarProps) => (
    <>
        <Label className="hardware__label" text={label}/>
        <Div className="hardware__bar">
            <Div className="hardware__used" style={{width: `${Math.min(1, used) * 100}%`}}/>
            {held ? <Div className="hardware__held" style={{width: `${Math.min(1, held) * 100}%`}}/> : null}
        </Div>
        <Label variant="secondary" className="hardware__value" text={text}/>
    </>
);

/** What the machine is using right now (CPU, memory, the GPU, the models' disk), what the loaded model holds,
 * and what llama.cpp itself sees: its own device list (the authority on whether the GPU is usable) */
export const HardwarePanel = ({status}: HardwarePanelProps) => {
    const [output, setOutput] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)
    const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null)
    const [folder, setFolder] = useState<ModelFolder | null>(null)

    useEffect(() => {
        let cancelled = false
        const read = () => getSystemSnapshot().then((s) => !cancelled && setSnapshot(s)).catch(() => undefined)
        void read()
        getModelFolder().then((f) => !cancelled && setFolder(f)).catch(() => undefined)
        const id = setInterval(() => void read(), POLL_MS)
        return () => {
            cancelled = true
            clearInterval(id)
        }
    }, [])

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
    const loaded = status?.state === 'ready'
    const serverVram = loaded && status?.placement?.memory ? status.placement.memory.vram_mib * MIB : null
    const gpu = snapshot?.gpu ?? null
    return (
        <Div className="models__section">
            <Label variant="secondary" className="models__heading" text="This machine"/>
            {snapshot ? (
                <Div className="hardware">
                    <LoadBar label="CPU" used={snapshot.cpu_percent / 100}
                             text={`${Math.round(snapshot.cpu_percent)}% of ${snapshot.cpu_threads} threads`}/>
                    <LoadBar label="Memory" used={snapshot.memory_used_bytes / snapshot.memory_total_bytes}
                             held={snapshot.server_memory_bytes != null ? snapshot.server_memory_bytes / snapshot.memory_total_bytes : undefined}
                             text={`${formatBytes(snapshot.memory_used_bytes)} of ${formatBytes(snapshot.memory_total_bytes)}`}/>
                    {gpu ? (
                        <>
                            <LoadBar label="GPU memory" used={gpu.vram_used_bytes / gpu.vram_total_bytes}
                                     held={serverVram != null ? serverVram / gpu.vram_total_bytes : undefined}
                                     text={`${formatBytes(gpu.vram_used_bytes)} of ${formatBytes(gpu.vram_total_bytes)}`}/>
                            {gpu.busy_percent != null ? (
                                <LoadBar label="GPU load" used={gpu.busy_percent / 100} text={`${gpu.busy_percent}%`}/>
                            ) : null}
                        </>
                    ) : null}
                    {folder?.free_bytes != null && folder.total_bytes ? (
                        <LoadBar label="Models disk" used={1 - folder.free_bytes / folder.total_bytes}
                                 text={`${formatBytes(folder.total_bytes - folder.free_bytes)} of ${formatBytes(folder.total_bytes)}`}/>
                    ) : null}
                </Div>
            ) : <Label variant="secondary" className="models__meta" text="Reading…"/>}
            {snapshot && !gpu ? (
                <Label variant="secondary" className="models__meta" text="The GPU's live figures are not available on this system (they come from the AMD driver on Linux)."/>
            ) : null}
            {snapshot ? (
                <Label variant="secondary" className="models__meta hardware__key"
                       text="Bright: held by the model server · dim: everything else"/>
            ) : null}

            {facts && loaded ? (
                <Div className="hardware__server">
                    <Label className="hardware__server-title" text={`Model server · ${status?.model ?? ''}`}/>
                    <Div className="hardware__facts">
                        <Div className="hardware__fact">
                            <Label variant="secondary" text="On the GPU"/>
                            <Label className="hardware__fact-value" text={serverVram != null ? formatBytes(serverVram) : '–'}/>
                        </Div>
                        <Div className="hardware__fact">
                            <Label variant="secondary" text="In RAM"/>
                            <Label className="hardware__fact-value" text={snapshot?.server_memory_bytes != null ? formatBytes(snapshot.server_memory_bytes) : '–'}/>
                        </Div>
                        <Div className="hardware__fact">
                            <Label variant="secondary" text="GPU temperature"/>
                            <Label className="hardware__fact-value" text={gpu?.temperature_c != null ? `${Math.round(gpu.temperature_c)} °C` : '–'}/>
                        </Div>
                        <Div className="hardware__fact">
                            <Label variant="secondary" text="GPU clock"/>
                            <Label className="hardware__fact-value" text={gpu?.clock_mhz != null ? `${(gpu.clock_mhz / 1000).toFixed(2)} GHz` : '–'}/>
                        </Div>
                    </Div>
                    {status?.placement ? (
                        <Label className={status.placement.verdict === 'gpu' || status.placement.verdict === 'unknown' ? 'models__meta' : 'models__error'}
                               text={status.placement.summary}/>
                    ) : null}
                    {facts.layers_offloaded != null ? (
                        <Label variant="secondary" className="models__meta" text={`${facts.layers_offloaded} of ${facts.layers_total} layers on the GPU`}/>
                    ) : null}
                </Div>
            ) : null}

            <SettingsRow label="Devices llama-server sees" help="If the GPU is missing here, the model runs on the CPU.">
                <Button variant="secondary" text={busy ? 'Asking…' : 'Check devices'} disabled={busy} onClicked={() => void check()}/>
            </SettingsRow>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            {output != null ? <pre className="models__pre">{output.trim() || 'llama-server printed nothing.'}</pre> : null}
        </Div>
    )
};
