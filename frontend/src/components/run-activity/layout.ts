import type {CallStatus, RunStep} from '../../api/agent/types.ts'
import type {PromptReading} from '../../hooks/use-prompt-reading.ts'
import {textWidth} from '../../tui-engine'
import {formatTokenCount} from '../../utils/format.ts'

/** What goes between the figures of a line */
export const SEPARATOR = ' · '
/** The marker before a step's label (`[x] `) and the gap between the longest label and the detail beside it */
const MARKER_CELLS = 4
const LABEL_GAP = 2
/** Where a step's detail starts once it has moved under its label, for want of room beside it */
export const STACKED_INDENT = 4
/** The narrowest bar worth drawing beside a step's figures; narrower, it goes on a row of its own */
const MIN_STEP_BAR_COLS = 10

export const generatedFigures = (call: CallStatus): string[] =>
    call.generation == null ? [] : [`${formatTokenCount(call.generation.tokens)} tokens`, `${call.generation.tokens_per_second.toFixed(1)} tok/s`]

/** What a finished call came to, for the line of its step: the prompt it read and the reply it wrote, each with its speed */
export const finishedFigures = (call: CallStatus): string[] => {
    const prompt = call.prompt
    const work = prompt == null ? 0 : Math.max(0, prompt.total - prompt.cached)
    const readSpeed = prompt != null && prompt.elapsed_ms > 0 ? ` at ${Math.round((work / prompt.elapsed_ms) * 1000)} tok/s` : ''
    return [
        work > 0 ? `${formatTokenCount(work)} read${readSpeed}` : null,
        call.generation != null ? `${formatTokenCount(call.generation.tokens)} written at ${call.generation.tokens_per_second.toFixed(1)} tok/s` : null,
    ].filter((part) => part != null)
}

/** What the title line says besides the title: the progress of the prompt, or the speed of the reply, with what the run has spent */
export const headline = (title: string, spent: string[], call: CallStatus | null, reading: PromptReading | null): string[] => {
    const parts = [title]
    if (reading != null) {
        parts.push(
            ...(reading.percent != null
                ? [`${Math.round(reading.percent)}%`, `${formatTokenCount(Math.round(reading.done))} / ${formatTokenCount(reading.work)} tokens`]
                : [`${formatTokenCount(reading.work)} tokens to read`]),
            ...(reading.cached > 0 ? [`${formatTokenCount(reading.cached)} cached`] : []),
            ...(reading.perSecond != null ? [`${Math.round(reading.perSecond)} tok/s`] : []),
            ...spent,
        )
    } else {
        parts.push(...spent)
        if (call?.generation != null) parts.push(`${call.generation.tokens_per_second.toFixed(1)} tok/s`)
    }
    return parts
}

/** Breaks `parts` into lines of at most `cols` cells, between parts when it can and between the words of one that is longer than a line; `cols` 0 is a canvas not measured yet: one line */
export const wrapParts = (parts: string[], cols: number): string[] => {
    if (cols <= 0) return [parts.join(SEPARATOR)]
    const lines: string[] = []
    let line = ''
    for (const part of parts) {
        const next = line ? line + SEPARATOR + part : part
        if (line && textWidth(next) > cols) {
            lines.push(line)
            line = ''
        } else if (line) {
            line = next
            continue
        }
        // Starts a line: a part wider than it goes on over several, the last of them taking the next part after it
        let current = ''
        for (const word of part.split(' ')) {
            const grown = current ? `${current} ${word}` : word
            if (current && textWidth(grown) > cols) {
                lines.push(current)
                current = word
            } else current = grown
        }
        line = current
    }
    if (line) lines.push(line)
    return lines
}

/** The column the steps put their details at, beside their labels: after the longest label */
export const detailColumn = (steps: RunStep[]): number => MARKER_CELLS + Math.max(0, ...steps.map((step) => textWidth(step.label))) + LABEL_GAP

/** Where a step's detail and bar go in the room there is: beside its label when the detail fits there, under it when not */
export interface StepLayout {
    rows: number
    detail: string[]
    /** Under the label, at `STACKED_INDENT`, when false beside it, at the list's detail column */
    stacked: boolean
    /** The bar's row relative to the step's first, and its column; null when the step has no bar */
    bar: { row: number; col: number } | null
    /** How far the bar says the work is, 0 to 100; empty until something says */
    percent: number
}

/** The progress of a step's own work, as text: `3 / 12`, or `3` for work with no known end */
const progressText = (step: RunStep): string | null =>
    step.progress == null ? null : step.progress.total == null ? `${step.progress.done}` : `${step.progress.done} / ${step.progress.total}`

/**
 * What a step says and how it is laid out in `cols` cells. `reading` is the smoothed progress of the step's model call
 * when it is the one running (the page's own estimate between the server's reports), `null` for any other.
 */
export const stepLayout = (step: RunStep, reading: PromptReading | null, cols: number, detailCol: number): StepLayout => {
    const running = step.state === 'running'
    const call = step.call
    const own = step.detail != null ? [step.detail] : []
    const progress = progressText(step)
    const detail = running
        ? reading != null
            ? [...own, `reading ${formatTokenCount(Math.round(reading.done))} / ${formatTokenCount(reading.work)}`]
            : call?.generation != null
                ? [...own, 'writing', ...generatedFigures(call)]
                : [...own, ...(progress != null ? [progress] : [])]
        : step.state === 'done'
            ? [...own, ...(call != null ? finishedFigures(call) : [])]
            : own
    // A running step has a bar while there is something for it to show: a prompt being read, or a count with an end
    const percent = reading?.percent ?? (step.progress?.total ? Math.min(100, (step.progress.done / step.progress.total) * 100) : 0)
    const wantsBar = running && call?.generation == null && (call != null || step.progress?.total != null)
    const text = detail.join(SEPARATOR)
    if (cols <= 0 || detailCol + textWidth(text) <= cols) {
        const barCol = detailCol + (text ? textWidth(text) + 2 : 0)
        if (!wantsBar) return {rows: 1, detail, stacked: false, bar: null, percent}
        return cols <= 0 || cols - barCol >= MIN_STEP_BAR_COLS
            ? {rows: 1, detail, stacked: false, bar: {row: 0, col: barCol}, percent}
            : {rows: 2, detail, stacked: false, bar: {row: 1, col: STACKED_INDENT}, percent}
    }
    const lines = wrapParts(detail, cols - STACKED_INDENT)
    return {rows: 1 + lines.length + (wantsBar ? 1 : 0), detail: lines, stacked: true, bar: wantsBar ? {row: 1 + lines.length, col: STACKED_INDENT} : null, percent}
}
