import {useEffect, useState} from 'react'

import '../styles/run-status.scss'
import type {TurnView} from '../hooks/use-turn.ts'
import {formatDurationShort, formatTokenCount} from '../utils/format.ts'
import {Button, Div, Label} from './primitives'
import {ThinkingAnimation} from './thinking-animation.tsx'

export interface RunStatusProps {
    view: TurnView
    onStop: () => void
}

/** Seconds since `since`, read from the clock on every tick rather than counted up, and again when the tab is shown: a background tab's timers are throttled, so a counter falls behind real time and a run that is still going looks stalled the moment you tab back in */
const useSecondsSince = (since: number | null): number => {
    const [now, setNow] = useState(() => Date.now())
    useEffect(() => {
        const tick = () => setNow(Date.now())
        const id = setInterval(tick, 1000)
        document.addEventListener('visibilitychange', tick)
        return () => {
            clearInterval(id)
            document.removeEventListener('visibilitychange', tick)
        }
    }, [])
    return since == null ? 0 : Math.max(0, Math.floor((now - since) / 1000))
}

/** Where the next reply will land while a run is going: what it is doing now (the model thinking, a tool running, or waiting for the user), how long the run has been going and what it has spent, with the stop button */
export function RunStatus({view, onStop}: RunStatusProps) {
    const elapsed = useSecondsSince(view.startedAt)
    const toolElapsed = useSecondsSince(view.toolStartedAt)
    const waiting = view.status === 'waiting_for_permission'

    const doing = waiting
        ? 'Waiting for your answer'
        : view.runningTool != null
            ? `Running ${view.runningTool} (${formatDurationShort(toolElapsed)})`
            : 'Thinking...'
    const spent = [
        view.startedAt != null ? formatDurationShort(elapsed) : null,
        view.evalTokens > 0 ? `${formatTokenCount(view.evalTokens)} tokens` : null,
        view.stepLimit != null && view.step > 0 ? `step ${view.step} of ${view.stepLimit}` : null,
    ].filter((part) => part != null).join(', ')

    return (
        <Div className="vbox run-status">
            <ThinkingAnimation isPlaying={!waiting}/>
            <Div className="run-status__line">
                <Label className="run-status__label" text={spent ? `${doing} - ${spent}` : doing}/>
                {waiting ? null : <Button variant="secondary" text="Stop" onClicked={onStop}/>}
            </Div>
        </Div>
    )
}
