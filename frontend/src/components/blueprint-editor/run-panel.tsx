import {useState} from 'react'
import {Frame} from '../frame'
import {Button, Div, Label} from '../primitives'
import {ConfirmPopup} from '../popups/base/confirm-popup'
import {Popup} from '../popups/base/popup'
import type {Allowance} from '../../api/agent/types'
import type {PendingTool, RunSummary} from '../../api/blueprints/types'
import type {useBlueprintRuns} from '../../hooks/blueprint-editor/use-blueprint-runs'
import {progressText, statusText} from '../../utils/blueprint-runs'

const summaryLine = (run: RunSummary, name: string) =>
    `${name} · ${run.scheduled_for ? 'Scheduled' : 'Manual'} · ${statusText(run.status)} · ${progressText(run.progress)}`

interface RunsPanelProps {
    blueprintId: number
    runs: ReturnType<typeof useBlueprintRuns>
    onOpen: (id: number) => void
}

export const RunsPanel = ({blueprintId, runs, onOpen}: RunsPanelProps) => {
    const [clearing, setClearing] = useState(false)
    const [removing, setRemoving] = useState<number | null>(null)
    const queued = runs.queue?.queued ?? []
    const running = runs.queue?.running ?? null
    return (
        <Div className="blueprint-editor__runs">
            <Frame title="Queue" actions={[]} active={false}>
                <Label variant="secondary" text="One queue for every blueprint."/>
                {running ? (
                    <Div className="blueprint-editor__run-row">
                        <Label text={summaryLine(running, runs.names[running.blueprint_id] ?? `Blueprint ${running.blueprint_id}`)}/>
                        {running.error ? <Label variant="secondary" text={running.error}/> : null}
                        <Div className="blueprint-editor__run-actions">
                            {running.blueprint_id !== blueprintId ? (
                                <Button variant="secondary" text="Open" disabled={runs.acting} onClicked={() => onOpen(running.blueprint_id)}/>
                            ) : null}
                            <Button variant="secondary" text="Stop" disabled={runs.acting} onClicked={() => { void runs.stop() }}/>
                        </Div>
                    </Div>
                ) : <Label variant="secondary" text="Nothing is running."/>}
                {queued.map((run, index) => (
                    <Div key={run.id} className="blueprint-editor__run-row">
                        <Label text={`${index + 1}. Run ${run.id} · ${summaryLine(run, runs.names[run.blueprint_id] ?? `Blueprint ${run.blueprint_id}`)}`}/>
                        <Div className="blueprint-editor__run-actions">
                            <Button variant="secondary" text="Up" disabled={runs.acting || index === 0} onClicked={() => { void runs.move(index, -1) }}/>
                            <Button variant="secondary" text="Down" disabled={runs.acting || index === queued.length - 1} onClicked={() => { void runs.move(index, 1) }}/>
                            <Button variant="secondary" text="Remove" disabled={runs.acting} onClicked={() => setRemoving(run.id)}/>
                            {run.blueprint_id !== blueprintId ? (
                                <Button variant="secondary" text="Open" disabled={runs.acting} onClicked={() => onOpen(run.blueprint_id)}/>
                            ) : null}
                        </Div>
                    </Div>
                ))}
            </Frame>
            <Button variant="secondary" text="Stop all" disabled={runs.acting || queued.length === 0} onClicked={() => setClearing(true)}/>
            <ConfirmPopup open={clearing} title="Stop all waiting runs" confirmLabel="Stop all"
                          message="Cancel all waiting runs. The active run and schedules continue."
                          onClose={() => setClearing(false)} onConfirm={() => { void runs.clear() }}/>
            <ConfirmPopup open={removing !== null} title="Remove from queue" confirmLabel="Remove"
                          message={removing === null ? '' : `Remove run ${removing} from the queue? The one already running stays, and the schedule stays on.`}
                          onClose={() => setRemoving(null)} onConfirm={() => { if (removing !== null) void runs.remove(removing) }}/>
        </Div>
    )
}

interface RunPermissionProps {
    pending: PendingTool
    busy: boolean
    onAnswer: (allowance: Allowance) => void
    onStop: () => void
}

export const RunPermission = ({pending, busy, onAnswer, onStop}: RunPermissionProps) => {
    const grant = pending.call.permission.status === 'denied' ? pending.call.permission.escalation : null
    return (
        <Div className="blueprint-editor__permission">
            <Label variant="secondary" text="PERMISSION NEEDED"/>
            <Label text={pending.call.name}/>
            <Label variant="secondary" text={JSON.stringify(pending.call.arguments)}/>
            {grant ? <Label text={grant.ui_message}/> : null}
            {pending.call.permission.status === 'denied' ? <Label variant="secondary" text={pending.call.permission.reason}/> : null}
            <Div className="blueprint-editor__run-actions">
                <Button variant="secondary" text="Don't allow" disabled={busy} onClicked={() => onAnswer('deny')}/>
                {grant ? <Button variant="secondary" text="Always in this chat" disabled={busy} onClicked={() => onAnswer('permanent')}/> : null}
                {grant ? <Button text="Allow once" disabled={busy} onClicked={() => onAnswer('only_now')}/> : null}
                <Button variant="secondary" text="Stop" disabled={busy} onClicked={onStop}/>
            </Div>
        </Div>
    )
}

interface RunStartPopupProps {
    open: boolean
    saving: boolean
    saveDisabled: boolean
    onClose: () => void
    onRunSaved: () => void
    onSaveAndRun: () => void
}

export const RunStartPopup = ({open, saving, saveDisabled, onClose, onRunSaved, onSaveAndRun}: RunStartPopupProps) => (
    <Popup open={open} title="Save before running" onClose={() => { if (!saving) onClose() }}
           actions={[{keys: ['Enter'], shown: 'enter', label: 'save and run', run: () => { if (!saving && !saveDisabled) onSaveAndRun() }}]}>
        <Label text="A run uses the saved blueprint. These edits are not in it yet."/>
        <Div className="popup__actions">
            <Button text="Cancel" variant="secondary" disabled={saving} onClicked={onClose}/>
            <Button text="Run saved version" variant="secondary" disabled={saving} onClicked={onRunSaved}/>
            <Button text={saving ? 'Saving…' : 'Save and run'} disabled={saving || saveDisabled} onClicked={onSaveAndRun}/>
        </Div>
    </Popup>
)
