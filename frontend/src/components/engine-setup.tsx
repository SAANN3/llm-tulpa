import {useEffect, useState, type CSSProperties} from 'react'
import '../styles/model-picker.scss'
import {getHardware, type Hardware} from '../api/runtime/hardware'
import {getInstallStatus, startInstall, type Channel, type Installed, type InstallTask} from '../api/runtime/install'
import {getDevices} from '../api/runtime/devices'
import {errorReason} from '../utils/error-reason.ts'
import {formatBytes} from '../utils/format-bytes.ts'
import {Button, Div, Input, Label, Select} from './primitives'

export interface EngineSetupProps {
    /** Called whenever the installed state is known */
    onInstalled: (installed: Installed | null) => void
}

const SOURCES: Record<string, Channel> = {
    'Tested release (recommended)': 'pinned',
    'Newest release (may break)': 'latest',
    'My own llama-server binary': 'custom',
}

const POLL_MS = 1000

/** Finds the right llama.cpp build for this machine and installs it: what the machine has, the build
 * to take, where to take it from, then the install itself with its progress and its outcome */
export const EngineSetup = ({onInstalled}: EngineSetupProps) => {
    const [hardware, setHardware] = useState<Hardware | null>(null)
    const [installed, setInstalled] = useState<Installed | null>(null)
    const [task, setTask] = useState<InstallTask | null>(null)
    const [build, setBuild] = useState<string>('')
    const [source, setSource] = useState(Object.keys(SOURCES)[0])
    const [customPath, setCustomPath] = useState('')
    const [devices, setDevices] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)

    const refresh = async () => {
        const status = await getInstallStatus()
        setInstalled(status.installed)
        setTask(status.task)
        onInstalled(status.installed)
        return status
    }

    useEffect(() => {
        getHardware().then((hw) => {
            setHardware(hw)
            setBuild((current) => current || hw.recommended)
        }).catch((e) => setError(errorReason(e, 'Could not look at this machine.')))
        refresh().catch(() => setError('Could not read the install state.'))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    const running = task?.state === 'running'
    useEffect(() => {
        if (!running) return
        const id = setInterval(() => void refresh().catch(() => undefined), POLL_MS)
        return () => clearInterval(id)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [running])

    const onInstall = async () => {
        setError(null)
        setDevices(null)
        try {
            setTask(await startInstall(SOURCES[source], build || undefined, customPath || undefined))
        } catch (e) {
            setError(errorReason(e, 'Could not start the install.'))
        }
    }

    const onCheck = async () => {
        try {
            setDevices(await getDevices())
        } catch (e) {
            setError(errorReason(e, 'Could not ask llama.cpp for its devices.'))
        }
    }

    const fraction = task && task.total_bytes > 0 ? Math.min(1, task.completed_bytes / task.total_bytes) : 0

    return (
        <Div className="setup__step">
            {hardware ? (
                <>
                    <Label className="setup__lead" text={`${hardware.cpu || 'CPU'} · ${Math.round(hardware.ram_mib / 1024)} GB RAM`}/>
                    {hardware.gpus.map((g, i) => (
                        <Label key={i} variant="secondary" className="field__help"
                               text={`GPU: ${g.name}${g.vram_mib ? ` · ${Math.round(g.vram_mib / 1024)} GB` : ''}`}/>
                    ))}
                    {hardware.gpus.length === 0 ? <Label variant="secondary" className="field__help" text="No GPU found: the model will run on the CPU."/> : null}
                    <Label variant="secondary" className="field__help" text={`Suggested build: ${hardware.recommended} — ${hardware.reason}`}/>
                    {hardware.support !== 'tested' ? (
                        <Label variant="secondary" className="field__help"
                               text="This system is untested. It may work; if it doesn't, please open an issue on GitHub with what you see."/>
                    ) : null}
                    {hardware.missing.map((m) => (
                        <Label key={m.name} className="model-picker__error" text={`Missing ${m.name}: ${m.hint}`}/>
                    ))}
                </>
            ) : null}

            {installed ? (
                <Label className="setup__lead" text={`Installed: llama.cpp ${installed.tag} (${installed.build}, ${installed.source})`}/>
            ) : null}

            <Div className="field">
                <Label className="field__label" text="Build"/>
                <Select values={hardware?.builds ?? []} selected={build} onChosen={setBuild}/>
            </Div>
            <Div className="field">
                <Label className="field__label" text="Source"/>
                <Select values={Object.keys(SOURCES)} selected={source} onChosen={setSource}/>
            </Div>
            {SOURCES[source] === 'custom' ? (
                <Input text={customPath} onChanged={setCustomPath} placeholder="/path/to/llama-server"/>
            ) : null}

            {running && task ? (
                <Div className="model-picker__info">
                    <Label variant="secondary" className="model-picker__meta"
                           text={`${task.phase}${task.total_bytes > 0 ? ` · ${formatBytes(task.completed_bytes)} / ${formatBytes(task.total_bytes)}` : ''}`}/>
                    <Div className="model-picker__bar">
                        <Div className="model-picker__bar-fill" style={{'--fraction': fraction} as CSSProperties}/>
                    </Div>
                </Div>
            ) : null}
            {task?.state === 'failed' ? <Label className="model-picker__error" text={task.error ?? 'The install failed.'}/> : null}
            {error ? <Label className="model-picker__error" text={error}/> : null}

            <Div className="popup__actions">
                <Button text={running ? 'Installing…' : installed ? 'Reinstall' : 'Install'} disabled={running || (SOURCES[source] === 'custom' && !customPath.trim())}
                        onClicked={() => void onInstall()}/>
                {installed ? <Button variant="secondary" text="Check devices" onClicked={() => void onCheck()}/> : null}
            </Div>
            {devices != null ? <pre className="models__pre">{devices.trim()}</pre> : null}
        </Div>
    )
};
