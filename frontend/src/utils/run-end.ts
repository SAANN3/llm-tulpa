import type {RunEnded, RunEndReason} from '../api/agent/types.ts'
import type {UnseenEnd} from '../api/chats/types.ts'
import {formatDurationShort, formatTokenCount} from './format.ts'

/**
 * The line shown at the end of a chat for a run that ended without an answer: how it ended, how long the run
 * took and what its model calls generated. An answered run needs none (the reply is there), and a wait for
 * permission has its own prompt. `stepLimit` is the user's current setting.
 */
export const describeRunEnd = (end: RunEnded, stepLimit: number | null): string | null => {
    const seconds = Math.max(0, (Date.parse(end.ended_at) - Date.parse(end.started_at)) / 1000)
    const spent = `${formatDurationShort(seconds)}${end.eval_tokens > 0 ? `, ${formatTokenCount(end.eval_tokens)} tokens` : ''}`
    switch (end.reason) {
        case 'stopped':
            return `Stopped after ${spent}`
        case 'step_limit':
            return `Reached the step limit${stepLimit != null ? ` of ${stepLimit}` : ''} after ${spent}. Send a message to continue.`
        case 'failed':
            return `Failed after ${spent}: ${end.detail ?? 'something went wrong'}`
        default:
            return null
    }
}

/** What a chat's row says about a run that just ended, until the chat is opened (a run the user stopped has nothing to say) */
export const unseenEndOf = (reason: RunEndReason): UnseenEnd | null => (reason === 'stopped' ? null : reason)
