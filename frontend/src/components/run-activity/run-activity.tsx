import {useEffect, useState} from 'react'
import type {CallStatus, RunStep, StepList, StepState} from '../../api/agent/types.ts'
import {usePromptReading, type PromptReading} from '../../hooks/use-prompt-reading.ts'
import {useSpeedSamples, type SpeedPoint} from '../../hooks/use-speed-samples.ts'
import {ROW_PX, TuiAreaGraph, TuiCanvas, TuiRenderMode, TuiScannerBar, TuiText, useTuiScene, type Level} from '../../tui-engine'
import {detailColumn, headline, SEPARATOR, STACKED_INDENT, stepLayout, wrapParts, type StepLayout} from './layout.ts'
import {speedSeries} from './speed.ts'

/** Height of the bar and the graph of a model call: three rows is a framed bar (a frame with the scan head drawn through it) */
const BAR_ROWS = 3

const STEP_MARKS: Record<StepState, { mark: string; level: Level }> = {
    pending: {mark: '[ ]', level: 2},
    running: {mark: '[>]', level: 5},
    done: {mark: '[x]', level: 4},
    failed: {mark: '[!]', level: 5},
    skipped: {mark: '[-]', level: 2},
}

export interface RunActivityProps {
    /** What the run is doing now, as a title; the title of the newest list while there are some */
    title: string
    /** How long it has gone on and what it has spent: said on the title line after the call's own figures */
    spent: string[]
    /** The run's own model call in flight, when there is one that isn't a step's */
    call: CallStatus | null
    /** The lists of steps of the work going on, oldest first, each drawn as a block under the one before; without any the canvas shows the one model call */
    lists: StepList[]
}

/** Tells the component around the canvas how many cells wide the canvas is: what it has to say is laid out in lines, and how many rows they take sets the canvas's height */
const WidthProbe = ({onCols}: { onCols: (cols: number) => void }) => {
    const {cols} = useTuiScene()
    useEffect(() => onCols(cols), [cols, onCols])
    return null
}

/**
 * What a run is doing, drawn on a canvas of its own: the model call reading its prompt (a bar) or generating (a graph
 * of its speed), or the lists of steps of the work going on as checklists, each with what it says and the bar of what it is
 * reading or counting; the figures of the call are on the title line. A line that doesn't fit the width goes on to a
 * next one. Before the server has said anything of the call the bar stands empty; a run with no model call going (a
 * tool running, a wait for the user) is only the title. Which step is which is none of its business: it draws whatever
 * steps it is given, so a new kind of work needs nothing here.
 */
export const RunActivity = ({title, spent, call, lists}: RunActivityProps) => {
    const working = lists.length > 0
    // One step is taken to run at a time: the first that does, in the newest list that has one, has the model call
    // whose progress is smoothed and graphed
    const running = [...lists].reverse().flatMap((list) => list.steps).find((step) => step.state === 'running') ?? null
    const live = working ? running?.call ?? null : call
    const samples = useSpeedSamples(live)
    const reading = usePromptReading(live)
    const [cols, setCols] = useState(0)
    const lines = wrapParts(headline(title, spent, working ? null : call, working ? null : reading), cols)
    // Lists after the first say what they are, so the blocks can be told apart
    const blocks = lists.map((list) => {
        const detailCol = detailColumn(list.steps)
        const layouts = list.steps.map((step) => stepLayout(step, step === running ? reading : null, cols, detailCol))
        return {list, detailCol, layouts, rows: (lists.length > 1 ? 1 : 0) + layouts.reduce((sum, layout) => sum + layout.rows, 0)}
    })
    const rows = lines.length + (working ? blocks.reduce((sum, block) => sum + block.rows, 0) : call != null ? BAR_ROWS : 0)
    let nextRow = lines.length

    return (
        <TuiCanvas
            className="run-activity"
            style={{height: rows * ROW_PX}}
            label={lines.join(' ')}
            renderMode={TuiRenderMode.Continuous}
            fps={30}
        >
            <WidthProbe onCols={setCols}/>
            {lines.map((line, i) => <TuiText key={i} text={line} x={0} y={i} w="100%" truncate/>)}
            {working
                ? blocks.map((block) => {
                    const first = nextRow
                    nextRow += block.rows
                    return <ListBlock key={block.list.id} block={block} row={first} titled={lists.length > 1}/>
                })
                : <CallRows call={call} reading={reading} samples={samples} row={lines.length}/>}
        </TuiCanvas>
    )
}

/** One list of steps: its title when there is more than one list, then its steps */
const ListBlock = ({block, row, titled}: { block: { list: StepList; detailCol: number; layouts: StepLayout[] }; row: number; titled: boolean }) => {
    let next = row + (titled ? 1 : 0)
    return (
        <>
            {titled ? <TuiText text={block.list.title} x={0} y={row} w="100%" truncate level={3}/> : null}
            {block.list.steps.map((step, i) => {
                const at = next
                next += block.layouts[i].rows
                return <StepRow key={step.id} step={step} row={at} layout={block.layouts[i]} detailCol={block.detailCol}/>
            })}
        </>
    )
}

/** The block of the one model call: the bar or the graph */
const CallRows = ({call, reading, samples, row}: { call: CallStatus | null; reading: PromptReading | null; samples: SpeedPoint[]; row: number }) => {
    if (call == null) return null
    // Before the server's first report there is nothing to show but an empty bar: a sweep across it would only be noise
    if (reading != null) return <TuiScannerBar x={0} y={row} w="100%" h={BAR_ROWS} value={reading.percent ?? 0} revealMode="color" showPercent={false}/>
    if (call.generation != null) {
        const {values, max} = speedSeries(samples)
        return <TuiAreaGraph x={0} y={row} w="100%" h={BAR_ROWS} values={values} max={max}/>
    }
    return <TuiScannerBar x={0} y={row} w="100%" h={BAR_ROWS} value={0} revealMode="color" showPercent={false}/>
}

/** One step of the checklist */
const StepRow = ({step, row, layout, detailCol}: { step: RunStep; row: number; layout: StepLayout; detailCol: number }) => {
    const {mark, level} = STEP_MARKS[step.state]
    const detailLevel = step.state === 'running' ? 3 : 2
    return (
        <>
            <TuiText text={`${mark} ${step.label}`} x={0} y={row} level={level}/>
            {layout.stacked
                ? layout.detail.map((line, i) => <TuiText key={i} text={line} x={STACKED_INDENT} y={row + 1 + i} w={`100% - ${STACKED_INDENT}`} truncate level={detailLevel}/>)
                : layout.detail.length
                    ? <TuiText text={layout.detail.join(SEPARATOR)} x={detailCol} y={row} w={`100% - ${detailCol}`} truncate level={detailLevel}/>
                    : null}
            {layout.bar != null
                ? <TuiScannerBar x={layout.bar.col} y={row + layout.bar.row} w={`100% - ${layout.bar.col}`} h={1} value={layout.percent} revealMode="color" showPercent={false}/>
                : null}
        </>
    )
}
