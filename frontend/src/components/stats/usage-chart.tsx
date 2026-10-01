import {useState} from 'react'
import '../../styles/usage-chart.scss'
import {Div, Label} from '../primitives'

export interface UsageChartProps {
    title: string
    /** One entry per bar, oldest first */
    bars: { day: string; value: number | null }[]
    format: (value: number) => string
}

/** A bar per day, scaled to the largest one (a day without a value is an empty bar); hovering a
 * bar reads out its day and value */
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
            <Label variant="secondary" className="usage-chart__readout"
                   text={shown ? `${shown.day} · ${shown.value == null ? '–' : format(shown.value)}` : ''}/>
        </Div>
    )
};
