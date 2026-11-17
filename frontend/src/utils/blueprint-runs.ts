import type {BlueprintNodeStatus, RunDetail, RunProgress} from '../api/blueprints/types'

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

export interface RunMark {status: BlueprintNodeStatus; caption: string}

/** Node highlights for a run of the revision that is open. A different snapshot stays in the run list. */
export const liveMarks = (live: RunDetail | null, revision: number): Record<string, RunMark> | null => {
    if (!live || live.run.revision !== revision) return null
    return Object.fromEntries(Object.entries(live.state.nodes).map(([id, node]) => {
        const step = node.iteration.map(item => item.index + 1).join('.')
        const caption = [step ? `iteration ${step}` : '', node.executions > 1 ? `${node.executions} runs` : ''].filter(Boolean).join(' · ')
        return [id, {status: node.status, caption}]
    }))
}
