import {getActivity} from '../../api/stats/activity'
import {Label} from '../../components/primitives'
import {Heatmap} from '../../components/stats/heatmap.tsx'
import {StatsStatus} from '../../components/stats/stats-status.tsx'
import {StatsTable} from '../../components/stats/stats-table.tsx'
import {UsageChart} from '../../components/stats/usage-chart.tsx'
import {useStats} from '../../hooks/use-stats.ts'
import type {RangeOption} from './ranges.ts'
import {formatMs} from '../../utils/stats.ts'

const KIND_NAMES: Record<string, string> = {process: 'Commands', agent: 'Sub-agents'}

/** When the user is active, chats started, and how background jobs went */
export const ActivityTab = ({range}: { range: RangeOption }) => {
    const {data, failed} = useStats(getActivity, range.query)
    if (failed || !data) return <StatsStatus failed={failed}/>

    return (
        <>
            <Heatmap days={data.days} view={range.heatmap}/>
            <UsageChart
                title="Chats started per day"
                bars={data.chats_per_day.map((day) => ({day: day.day, value: day.chats}))}
                format={String}
            />
            <Label text="Background jobs"/>
            <StatsTable
                sortable={false}
                columns={['Kind', 'Started', 'Succeeded', 'Failed', 'Killed', 'Lost', 'Average']}
                rows={data.jobs.map((job) => ({
                    key: job.kind,
                    cells: [
                        KIND_NAMES[job.kind] ?? job.kind,
                        String(job.total),
                        String(job.succeeded),
                        String(job.failed),
                        String(job.killed),
                        String(job.lost),
                        formatMs(job.average_seconds == null ? null : job.average_seconds * 1000),
                    ],
                    values: [],
                }))}
            />
        </>
    )
};
