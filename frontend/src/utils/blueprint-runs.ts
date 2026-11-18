import type {BlueprintNodeStatus, NodeProgress, RunDetail, RunProgress, Workflow} from '../api/blueprints/types'

const STATUS: Record<string, string> = {
    queued: 'Queued', running: 'Running', waiting_for_permission: 'Waiting for permission', stopping: 'Stopping',
    completed: 'Completed', failed: 'Failed', cancelled: 'Cancelled', interrupted: 'Interrupted',
    pending: 'Pending', skipped: 'Skipped',
}

export const statusText = (status: string) => STATUS[status] ?? status

export const progressText = (progress: RunProgress): string => {
    const counts = progress.total == null
        ? `${progress.completed} completed, ${progress.skipped} skipped`
        : `${progress.completed} of ${progress.total}${progress.skipped ? `, ${progress.skipped} skipped` : ''}`
    return progress.percent == null ? counts : `${counts} (${Math.round(progress.percent)}%)`
}

export interface RunMark {status: BlueprintNodeStatus; caption: string; preview?: string; html?: string}

export const displayedOutputs = (node: NodeProgress) => node.last_outputs ?? node.outputs

const kindJson = (kind: Workflow['nodes'][number]['kind']) => JSON.stringify(kind, (_key, value: unknown) => {
    if (value === null || typeof value !== 'object' || Array.isArray(value)) return value
    const fields = value as Record<string, unknown>
    return Object.fromEntries(Object.keys(fields).sort().map(name => [name, fields[name]]))
})

/** Layout and display names can change without invalidating a run's results. */
export const sameExecutionGraph = (current: Workflow, snapshot: Workflow): boolean => {
    if (current.schema_version !== snapshot.schema_version || current.nodes.length !== snapshot.nodes.length
        || current.edges.length !== snapshot.edges.length) return false
    return current.nodes.every(({id, kind}, index) => {
        const previous = snapshot.nodes[index]
        return id === previous.id && (kind === previous.kind || kindJson(kind) === kindJson(previous.kind))
    }) && current.edges.every(({id, from, to, port}, index) => {
        const previous = snapshot.edges[index]
        return id === previous.id && from === previous.from && to === previous.to && port === previous.port
    })
}

export const runMarks = (run: RunDetail | null): Record<string, RunMark> | null => {
    if (!run) return null
    const htmlIds = new Set(run.workflow.nodes.filter(node => node.kind.type === 'html').map(node => node.id))
    const printIds = new Set(run.workflow.nodes.filter(node => node.kind.type === 'print').map(node => node.id))
    return Object.fromEntries(Object.entries(run.state.nodes).map(([id, node]) => {
        const step = node.iteration.map(item => item.index + 1).join('.')
        const caption = [step ? `iteration ${step}` : '', node.executions > 1 ? `${node.executions} runs` : ''].filter(Boolean).join(' · ')
        const outputs = displayedOutputs(node)
        const value = printIds.has(id) ? outputs.value : undefined
        const text = value === undefined ? undefined : typeof value === 'string' && value.length ? value : JSON.stringify(value)
        const preview = text && text.length > 120 ? `${text.slice(0, 120)}…` : text
        const html = htmlIds.has(id) && typeof outputs.value === 'string' ? outputs.value : undefined
        return [id, {status: node.status, caption, preview, html}]
    }))
}
