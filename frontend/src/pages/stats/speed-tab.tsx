import {getUsageStats} from '../../api/stats/usage'
import type {UsageDay} from '../../api/stats/types'
import {Div, Label} from '../../components/primitives'
import {StatCard} from '../../components/stats/stat-card.tsx'
import {StatsStatus} from '../../components/stats/stats-status.tsx'
import {UsageChart} from '../../components/stats/usage-chart.tsx'
import {useStats} from '../../hooks/use-stats.ts'
import type {RangeOption} from './ranges.ts'
import {formatMs, formatPercent, formatSpeed, tokensPerSecond} from '../../utils/stats.ts'

/** How fast the model generates and reads prompts, and how long calls take */
export const SpeedTab = ({range}: { range: RangeOption }) => {
    const {data, failed} = useStats(getUsageStats, range.query)
    if (failed || !data) return <StatsStatus failed={failed}/>

    const sum = (pick: (day: UsageDay) => number) => data.days.reduce((total, day) => total + pick(day), 0)
    const replies = sum((day) => day.replies)
    if (replies === 0) return <Label variant="secondary" text="No usage in this period."/>

    const timed = sum((day) => day.timed_replies)
    const slow = sum((day) => day.slow_calls)
    const generation = tokensPerSecond(sum((day) => day.timed_eval_tokens), sum((day) => day.eval_ms))
    const prompt = tokensPerSecond(sum((day) => day.processed_tokens), sum((day) => day.processed_ms))
    const loadMs = sum((day) => day.load_ms)

    return (
        <>
            <Div className="stats__cards">
                <StatCard label="Generation" value={formatSpeed(generation)} hint={`${timed} of ${replies} replies timed`}/>
                <StatCard label="Prompt processing" value={formatSpeed(prompt)}
                          hint={`${sum((day) => day.processed_calls)} calls measured`}/>
                <StatCard label="Slow calls" value={String(slow)} hint={`${formatPercent(slow, replies)} of replies, 30s or more`}/>
                {/* llama.cpp reports no load time, so its replies record 0: a total of 0 means nothing was measured,
                    since even a cached load takes a few milliseconds */}
                <StatCard label="Loading models" value={loadMs > 0 ? formatMs(loadMs) : '–'}
                          hint={loadMs > 0 ? undefined : 'not measured with llama.cpp'}/>
            </Div>
            {timed === 0 ? (
                <Label variant="secondary" className="stats__note"
                       text="Speeds are measured from replies recorded since timings were added; there are none in this range yet."/>
            ) : null}
            <Div className="stats__charts">
                <UsageChart
                    title="Generation speed per day"
                    bars={data.days.map((day) => ({day: day.day, value: tokensPerSecond(day.timed_eval_tokens, day.eval_ms)}))}
                    format={formatSpeed}
                />
                <UsageChart
                    title="Prompt processing speed per day"
                    bars={data.days.map((day) => ({day: day.day, value: tokensPerSecond(day.processed_tokens, day.processed_ms)}))}
                    format={formatSpeed}
                />
                <UsageChart
                    title="Median call time per day"
                    bars={data.days.map((day) => ({day: day.day, value: day.call_ms_median}))}
                    format={formatMs}
                />
                <UsageChart
                    title="Slow calls per day"
                    bars={data.days.map((day) => ({day: day.day, value: day.slow_calls}))}
                    format={String}
                />
                </Div>
            <Label variant="secondary" className="stats__note"
                   text="A call is the whole round trip; a slow one is almost always a long prompt the server evaluated again instead of finding it cached. Prompt speed counts only tokens actually evaluated, which llama.cpp reports and Ollama doesn't."/>
        </>
    )
};
