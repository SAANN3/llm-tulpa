import {getBreakdown} from '../../api/stats/breakdown'
import {Label} from '../../components/primitives'
import {StatsStatus} from '../../components/stats/stats-status.tsx'
import {StatsTable} from '../../components/stats/stats-table.tsx'
import {useStats} from '../../hooks/use-stats.ts'
import type {RangeOption} from './ranges.ts'
import {formatPercent} from '../../utils/stats.ts'

const TOP_TOOLS = 8

/** How often each tool was called and how those calls ended */
export const ToolsTab = ({range}: { range: RangeOption }) => {
    const {data, failed} = useStats(getBreakdown, range.query)
    if (failed || !data) return <StatsStatus failed={failed}/>
    if (data.tools.length === 0) return <Label variant="secondary" text="No tool calls in this period."/>

    return (
        <StatsTable
            columns={['Tool', 'Calls', 'Failed', 'Denied', 'Failure rate']}
            initialSort={{column: 1, descending: true}}
            limit={TOP_TOOLS}
            rows={data.tools.map((tool) => ({
                key: tool.tool,
                cells: [
                    tool.tool,
                    String(tool.calls),
                    String(tool.failed),
                    String(tool.denied),
                    formatPercent(tool.failed, tool.calls),
                ],
                values: [tool.tool, tool.calls, tool.failed, tool.denied, tool.calls > 0 ? tool.failed / tool.calls : 0],
            }))}
        />
    )
};
