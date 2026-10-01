import {useState} from 'react'
import {useNavigate} from 'react-router-dom'
import '../styles/stats.scss'
import {Button, Div, Label} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {UsageChart} from '../components/usage-chart.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useUsageStats} from '../hooks/use-stats.ts'
import {formatTokenCount} from '../utils/format.ts'

const RANGES = [7, 30, 90]
const DAY_MS = 24 * 60 * 60 * 1000

/** The last `count` UTC days ending today, oldest first — the backend's day boundary */
const lastDays = (count: number): string[] =>
    Array.from({length: count}, (_, i) => new Date(Date.now() - (count - 1 - i) * DAY_MS).toISOString().slice(0, 10));

const Stats = () => {
    useDocumentTitle('Usage')
    const navigate = useNavigate()
    const [range, setRange] = useState(30)
    const {stats, loadedDays, failed} = useUsageStats(range)

    // The backend omits days without activity; the charts want every day of the range
    const byDay = new Map(stats?.days.map((day) => [day.day, day]))
    const days = lastDays(loadedDays ?? range)
    const sum = (pick: (day: NonNullable<ReturnType<typeof byDay.get>>) => number) =>
        (stats?.days ?? []).reduce((total, day) => total + pick(day), 0)

    return (
        <Div className="page center vbox stats">
            <TypewriterLabel className="stats__title" text="[ Usage ]" charIntervalMs={30}/>
            <Div className="dos-frame stats__panel">
                <span className="dos-frame__title">Token usage</span>
                <Div className="dos-frame__body stats__body">
                    <Div className="stats__ranges">
                        {RANGES.map((days) => (
                            <Button key={days} variant={days === range ? 'primary' : 'secondary'}
                                    text={`${days} days`} onClicked={() => setRange(days)}/>
                        ))}
                    </Div>

                    {failed ? (
                        <Label text="Couldn't load the usage."/>
                    ) : stats == null ? (
                        <Label variant="secondary" text="Loading…"/>
                    ) : stats.days.length === 0 ? (
                        <Label variant="secondary" text="No usage in this period."/>
                    ) : (
                        <>
                            <Div className="stats__totals">
                                <Div className="stats__total">
                                    <Label variant="secondary" text="Replies"/>
                                    <Label className="stats__total-value" text={String(sum((d) => d.replies))}/>
                                </Div>
                                <Div className="stats__total">
                                    <Label variant="secondary" text="Generated tokens"/>
                                    <Label className="stats__total-value" text={formatTokenCount(sum((d) => d.eval_tokens))}/>
                                </Div>
                                <Div className="stats__total">
                                    <Label variant="secondary" text="Prompt tokens"/>
                                    <Label className="stats__total-value" text={formatTokenCount(sum((d) => d.prompt_tokens))}/>
                                </Div>
                            </Div>
                            <UsageChart
                                title="Generated tokens per day"
                                bars={days.map((day) => ({day, value: byDay.get(day)?.eval_tokens ?? 0}))}
                                format={formatTokenCount}
                            />
                            <UsageChart
                                title="Replies per day"
                                bars={days.map((day) => ({day, value: byDay.get(day)?.replies ?? 0}))}
                                format={String}
                            />
                            <Label variant="secondary" className="stats__note"
                                   text="Days are UTC. The prompt is the whole context, sent again with every call, so prompt tokens grow with chat length rather than with new text."/>
                        </>
                    )}

                    <Button variant="secondary" text="Back" onClicked={() => navigate('/')}/>
                </Div>
            </Div>
        </Div>
    )
};

export default Stats
