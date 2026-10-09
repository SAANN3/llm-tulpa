import {getUsageStats} from '../../api/stats/usage'
import type {UsageDay} from '../../api/stats/types'
import {Div, Label} from '../../components/primitives'
import {StatCard} from '../../components/stats/stat-card.tsx'
import {StatsStatus} from '../../components/stats/stats-status.tsx'
import {UsageChart} from '../../components/stats/usage-chart.tsx'
import {useStats} from '../../hooks/use-stats.ts'
import type {RangeOption} from './ranges.ts'
import {formatTokenCount} from '../../utils/format.ts'
import {formatMs} from '../../utils/stats.ts'

/** Replies and tokens over the range */
export const UsageTab = ({range}: { range: RangeOption }) => {
    const {data, failed} = useStats(getUsageStats, range.query)
    if (failed || !data) return <StatsStatus failed={failed}/>

    const sum = (pick: (day: UsageDay) => number) => data.days.reduce((total, day) => total + pick(day), 0)
    const replies = sum((day) => day.replies)
    if (replies === 0) return <Label variant="secondary" text="No usage in this period."/>

    const generated = sum((day) => day.eval_tokens)
    const prompt = sum((day) => day.prompt_tokens)
    // The part of a prompt the server actually read (not served from its cache), and how long that took
    const processedCalls = sum((day) => day.processed_calls)
    const readMs = processedCalls > 0 ? sum((day) => day.processed_ms) / processedCalls : null

    return (
        <>
            <Div className="stats__cards">
                <StatCard label="Replies" value={String(replies)}
                          hint={data.call_ms_mean != null ? `${formatMs(data.call_ms_mean)} each on average` : undefined}/>
                <StatCard label="Generated tokens" value={formatTokenCount(generated)}
                          hint={`${formatTokenCount(Math.round(generated / replies))} per reply`}/>
                <StatCard label="Prompt tokens" value={formatTokenCount(prompt)}
                          hint={`${formatTokenCount(Math.round(prompt / replies))} per reply${readMs != null ? ` · ${formatMs(readMs)} to read` : ''}`}/>
            </Div>
            <Div className="stats__charts">
                <UsageChart
                    title="Generated tokens per day"
                    bars={data.days.map((day) => ({day: day.day, value: day.eval_tokens}))}
                    format={formatTokenCount}
                />
                <UsageChart
                    title="Replies per day"
                    bars={data.days.map((day) => ({day: day.day, value: day.replies}))}
                    format={String}
                />
            </Div>
            <Label variant="secondary" className="stats__note"
                   text="The prompt is the whole context, sent again with every call, so prompt tokens grow with chat length rather than with new text. “To read” is the time the server spent on the part of it that wasn't cached."/>
        </>
    )
};
