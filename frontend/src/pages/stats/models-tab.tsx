import {getBreakdown} from '../../api/stats/breakdown'
import {Label} from '../../components/primitives'
import {StatsStatus} from '../../components/stats/stats-status.tsx'
import {StatsTable} from '../../components/stats/stats-table.tsx'
import {useStats} from '../../hooks/use-stats.ts'
import type {RangeOption} from './ranges.ts'
import {formatTokenCount} from '../../utils/format.ts'
import {formatMs, formatSpeed, tokensPerSecond} from '../../utils/stats.ts'

const TOP_MODELS = 8

/** Replies, tokens and speed per model */
export const ModelsTab = ({range}: { range: RangeOption }) => {
    const {data, failed} = useStats(getBreakdown, range.query)
    if (failed || !data) return <StatsStatus failed={failed}/>
    if (data.models.length === 0) return <Label variant="secondary" text="No usage in this period."/>

    return (
        <StatsTable
            columns={['Model', 'Replies', 'Generated', 'Speed', 'Median call', 'Chats']}
            initialSort={{column: 1, descending: true}}
            limit={TOP_MODELS}
            rows={data.models.map((model) => {
                const speed = tokensPerSecond(model.timed_eval_tokens, model.eval_ms)
                return {
                    key: `${model.provider}/${model.model}`,
                    cells: [
                        model.model,
                        String(model.replies),
                        formatTokenCount(model.eval_tokens),
                        formatSpeed(speed),
                        formatMs(model.call_ms_median),
                        String(model.chats),
                    ],
                    values: [model.model, model.replies, model.eval_tokens, speed ?? -1, model.call_ms_median ?? -1, model.chats],
                }
            })}
        />
    )
};
