import {useState} from 'react'
import type {ActivityDay} from '../../api/stats/types'
import {Div, Label} from '../primitives'

/** How the heatmap draws a range: a cell is an hour, four hours, or a whole day */
export type HeatmapView = 'hours' | 'blocks' | 'months'

const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
const BLOCK_HOURS = 4

/** A `YYYY-MM-DD` day as its parts, with the weekday (Monday = 0): the weekday of a calendar date
 * doesn't depend on any timezone, so the UTC date is enough to find it */
const parts = (day: string) => {
    const [year, month, date] = day.split('-').map(Number)
    return {year, month, date, weekday: (new Date(Date.UTC(year, month - 1, date)).getUTCDay() + 6) % 7}
};

const hourLabel = (hour: number) => `${String(hour % 24).padStart(2, '0')}:00`;

/** The cell's fill: the accent, as strong as the count is against the busiest cell */
const heat = (count: number, max: number) =>
    ({background: `color-mix(in srgb, var(--color-tertiary) ${(count / max) * 100}%, transparent)`});

interface ViewProps {
    days: ActivityDay[]
    onHover: (text: string | null) => void
}

/** Seven days or fewer: a row per day, a cell per hour */
const HourGrid = ({days, onHover}: ViewProps) => {
    const max = Math.max(...days.flatMap((day) => day.hours), 1)

    return (
        <Div className="heatmap">
            {days.map((day) => {
                const {weekday, date} = parts(day.day)
                return (
                    <Div key={day.day} className="heatmap__row">
                        <Label variant="secondary" className="heatmap__label" text={`${WEEKDAYS[weekday]} ${date}`}/>
                        {day.hours.map((count, hour) => (
                            <Div key={hour} className="heatmap__cell" style={heat(count, max)}
                                 onHover={(over) => onHover(over
                                     ? `${WEEKDAYS[weekday]} ${date}, ${hourLabel(hour)} to ${hourLabel(hour + 1)} · ${count} messages`
                                     : null)}/>
                        ))}
                    </Div>
                )
            })}
            <Div className="heatmap__row">
                <Div className="heatmap__label"/>
                <Div className="heatmap__ticks">
                    {[0, 6, 12, 18, 24].map((hour) => (
                        <Label key={hour} variant="secondary" className={`heatmap__tick heatmap__tick--${hour === 0 ? 'start' : hour === 24 ? 'end' : 'mid'}`}
                               style={{left: `${(hour / 24) * 100}%`}} text={hour === 24 ? '24 H' : String(hour)}/>
                    ))}
                </Div>
            </Div>
        </Div>
    )
};

/** About a month: a column per day, a cell per four hours of it */
const BlockGrid = ({days, onHover}: ViewProps) => {
    const blocks = 24 / BLOCK_HOURS
    const sums = days.map((day) =>
        Array.from({length: blocks}, (_, block) =>
            day.hours.slice(block * BLOCK_HOURS, (block + 1) * BLOCK_HOURS).reduce((total, count) => total + count, 0)),
    )
    const max = Math.max(...sums.flat(), 1)

    return (
        <Div className="heatmap">
            {Array.from({length: blocks}, (_, block) => (
                <Div key={block} className="heatmap__row">
                    <Label variant="secondary" className="heatmap__label"
                           text={`${String(block * BLOCK_HOURS).padStart(2, '0')}-${String((block + 1) * BLOCK_HOURS).padStart(2, '0')}`}/>
                    {days.map((day, index) => {
                        const {weekday, date, month} = parts(day.day)
                        return (
                            <Div key={day.day} className="heatmap__cell heatmap__cell--tall" style={heat(sums[index][block], max)}
                                 onHover={(over) => onHover(over
                                     ? `${WEEKDAYS[weekday]} ${date} ${MONTHS[month - 1]}, ${hourLabel(block * BLOCK_HOURS)} to ${hourLabel((block + 1) * BLOCK_HOURS)} · ${sums[index][block]} messages`
                                     : null)}/>
                        )
                    })}
                </Div>
            ))}
            <Div className="heatmap__row">
                <Div className="heatmap__label"/>
                {days.map((day, index) => {
                    const {date, month} = parts(day.day)
                    // The month's name goes under the first day shown and under every 1st
                    const newMonth = index === 0 || date === 1
                    return (
                        <Div key={day.day} className="heatmap__day">
                            <Label variant="secondary" className="heatmap__day-number" text={String(date)}/>
                            <Label variant="secondary" className="heatmap__day-month" text={newMonth ? MONTHS[month - 1] : ''}/>
                        </Div>
                    )
                })}
            </Div>
        </Div>
    )
};

/** Whole months: a calendar per month side by side, a cell per day */
const MonthCalendars = ({days, onHover}: ViewProps) => {
    const totals = days.map((day) => day.hours.reduce((total, count) => total + count, 0))
    const max = Math.max(...totals, 1)

    const months: { key: string; year: number; month: number; days: { day: ActivityDay; total: number }[] }[] = []
    days.forEach((day, index) => {
        const {year, month} = parts(day.day)
        const key = `${year}-${month}`
        let entry = months[months.length - 1]
        if (entry?.key !== key) {
            entry = {key, year, month, days: []}
            months.push(entry)
        }
        entry.days.push({day, total: totals[index]})
    })

    return (
        <Div className="heatmap heatmap--months">
            {months.map((entry) => {
                // Blank cells before the 1st, so every date sits under its weekday
                const lead = parts(entry.days[0].day.day).weekday
                return (
                    <Div key={entry.key} className="heatmap__month">
                        <Label className="heatmap__month-title" text={`${MONTHS[entry.month - 1]} ${entry.year}`}/>
                        <Div className="heatmap__calendar">
                            {WEEKDAYS.map((weekday) => (
                                <Label key={weekday} variant="secondary" className="heatmap__weekday" text={weekday.slice(0, 2)}/>
                            ))}
                            {Array.from({length: lead}, (_, i) => <Div key={`blank-${i}`}/>)}
                            {entry.days.map(({day, total}) => {
                                const {weekday, date} = parts(day.day)
                                return (
                                    <Div key={day.day} className="heatmap__date" style={heat(total, max)}
                                         onHover={(over) => onHover(over
                                             ? `${WEEKDAYS[weekday]} ${date} ${MONTHS[entry.month - 1]} · ${total} messages`
                                             : null)}>
                                        <Label variant="secondary" className="heatmap__date-number" text={String(date)}/>
                                    </Div>
                                )
                            })}
                        </Div>
                    </Div>
                )
            })}
        </Div>
    )
};

export interface HeatmapProps {
    /** Every day of the range, oldest first */
    days: ActivityDay[]
    view: HeatmapView
}

const CAPTIONS: Record<HeatmapView, string> = {
    hours: 'Your messages: a cell is one hour.',
    blocks: 'Your messages: a cell is four hours of one day.',
    months: 'Your messages: a cell is one day.',
}

/** The user's activity over a range, drawn with a cell size that suits it; hovering a cell reads it out */
export const Heatmap = ({days, view}: HeatmapProps) => {
    const [hovered, setHovered] = useState<string | null>(null)
    const props = {days, onHover: setHovered}

    return (
        <Div className="heatmap-block">
            {view === 'hours' ? <HourGrid {...props}/> : view === 'blocks' ? <BlockGrid {...props}/> : <MonthCalendars {...props}/>}
            <Label variant="secondary" className="heatmap__readout" text={hovered ?? CAPTIONS[view]}/>
        </Div>
    )
};
