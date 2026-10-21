import '../styles/run-status.scss'
import type {TurnView} from '../hooks/use-turn.ts'
import {useSecondsSince} from '../hooks/use-seconds-since.ts'
import {formatDurationShort, formatTokenCount} from '../utils/format.ts'
import {Button, Div} from './primitives'
import {RunActivity} from './run-activity/run-activity.tsx'

export interface RunStatusProps {
    view: TurnView
    /** The reply being written has got to its text: the thinking is over */
    writing: boolean
    onStop: () => void
}

/** Where the next reply will land while a run is going: what it is doing now (reading the chat, the model thinking or writing, a compaction, a tool running, or waiting for the user), how long the run has been going and what it has spent, with the stop button */
export function RunStatus({view, writing, onStop}: RunStatusProps) {
    const elapsed = useSecondsSince(view.startedAt)
    const toolElapsed = useSecondsSince(view.toolStartedAt)
    const waiting = view.status === 'waiting_for_permission'
    const reading = view.call?.prompt != null && view.call.generation == null

    const doing = waiting
        ? 'Waiting for your answer'
        : view.runningTool != null
            ? `Running ${view.runningTool} (${formatDurationShort(toolElapsed)})`
            : view.steps.length > 0
                ? view.steps[view.steps.length - 1].title
                : reading
                    ? 'Reading the chat'
                    : view.call?.generation != null
                        ? writing ? 'Writing the reply' : 'Thinking'
                        : 'Thinking...'
    // evalTokens only grows when a step ends: the reply being written counts too, so the total is live
    const written = view.evalTokens + (view.call?.kind === 'reply' ? view.call.generation?.tokens ?? 0 : 0)
    const spent = [
        view.startedAt != null ? formatDurationShort(elapsed) : null,
        written > 0 ? `${formatTokenCount(written)} tokens` : null,
        view.stepLimit != null && view.step > 0 ? `step ${view.step} of ${view.stepLimit}` : null,
    ].filter((part) => part != null)

    return (
        <Div className="vbox run-status">
            {/* Keeps moving while it waits for the user too: the run is still open, only paused on their answer */}
            <RunActivity title={doing} spent={spent} call={waiting || view.runningTool != null ? null : view.call} lists={waiting ? [] : view.steps}/>
            {waiting ? null : <Button className="run-status__stop" variant="secondary" text="Stop" onClicked={onStop}/>}
        </Div>
    )
}
