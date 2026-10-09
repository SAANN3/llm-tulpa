import {useState} from 'react'
import '../../styles/usage-chart.scss'
import {Div, Label} from '../primitives'

export interface UsageChartProps {
    title: string
    /** One entry per bar, oldest first */
    bars: { day: string; value: number | null }[]
    format: (value: number) => string
}

/** A `YYYY-MM-DD` day as `Oct 9` */
const shortDay = (day: string) => {
    const [year, month, date] = day.split('-').map(Number)
    return new Date(year, month - 1, date).toLocaleDateString(undefined, {month: 'short', day: 'numeric'})
};

/** A bar per day, scaled to the largest one (a day without a value is an empty bar). Under it the first day on the
 * left and, on the right, the last day's value, or the hovered bar's */
export const UsageChart = ({title, bars, format}: UsageChartProps) => {
    const [hovered, setHovered] = useState<number | null>(null)
    const max = Math.max(...bars.map((bar) => bar.value ?? 0), 1)
    const shown = bars[hovered ?? bars.length - 1]

    return (
        <Div className="usage-chart">
            <Label text={title}/>
            <Div className="usage-chart__bars">
                {bars.map((bar, index) => (
                    <Div key={bar.day} className="usage-chart__slot" onHover={(over) => setHovered(over ? index : null)}>
                        <Div className="usage-chart__bar" style={{height: `${((bar.value ?? 0) / max) * 100}%`}}/>
                    </Div>
                ))}
            </Div>
            <Div className="usage-chart__axis">
                <Label variant="secondary" className="usage-chart__readout" text={bars.length > 1 ? shortDay(bars[0].day) : ''}/>
                <Label variant="secondary" className="usage-chart__readout"
                       text={shown ? `${shortDay(shown.day)} · ${shown.value == null ? '–' : format(shown.value)}` : ''}/>
            </Div>
        </Div>
    )
};
