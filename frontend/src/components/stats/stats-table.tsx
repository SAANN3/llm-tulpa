import {useState} from 'react'
import {ChevronDown, ChevronUp} from 'pixelarticons/react'
import {Button, Div, Label} from '../primitives'

export interface StatsRow {
    /** Unique among the rows */
    key: string
    /** What each column shows */
    cells: string[]
    /** What each column sorts by: the number behind the text, or the text itself */
    values: (number | string)[]
}

export interface StatsTableProps {
    columns: string[]
    rows: StatsRow[]
    /** Whether clicking a column header sorts by it (default true) */
    sortable?: boolean
    /** The column and direction the table starts sorted by (default: the first column, ascending) */
    initialSort?: { column: number; descending: boolean }
    /** Show only this many rows until "Show all" is pressed */
    limit?: number
}

const compare = (a: number | string, b: number | string): number =>
    typeof a === 'number' && typeof b === 'number' ? a - b : String(a).localeCompare(String(b));

/** A table of numbers. The first column is text and takes twice the width of the others; a click on
 * a column header sorts by it, and again flips the direction. */
export const StatsTable = ({columns, rows, sortable = true, initialSort, limit}: StatsTableProps) => {
    const [sort, setSort] = useState(initialSort ?? {column: 0, descending: false})
    const [expanded, setExpanded] = useState(false)
    const template = `minmax(0, 2fr) repeat(${columns.length - 1}, minmax(0, 1fr))`

    const sorted = sortable
        ? [...rows].sort((a, b) => (sort.descending ? -1 : 1) * compare(a.values[sort.column], b.values[sort.column]))
        : rows
    const shown = limit != null && !expanded ? sorted.slice(0, limit) : sorted

    const onHeader = (column: number) => {
        if (!sortable) return
        // The first click on a number column puts the biggest first, which is what people look for
        setSort((current) =>
            current.column === column
                ? {column, descending: !current.descending}
                : {column, descending: column > 0},
        )
    }

    return (
        <Div className="stats-table">
            <Div className="stats-table__row stats-table__row--head" style={{gridTemplateColumns: template}}>
                {columns.map((column, index) => (
                    <Div key={column}
                         className={`stats-table__head${index > 0 ? ' stats-table__head--number' : ''}${sortable ? ' stats-table__head--sortable' : ''}`}
                         onClick={() => onHeader(index)}>
                        <Label variant="secondary" text={column}/>
                        {sortable && sort.column === index
                            ? (sort.descending ? <ChevronDown width={12} height={12}/> : <ChevronUp width={12} height={12}/>)
                            : null}
                    </Div>
                ))}
            </Div>
            {shown.map((row) => (
                <Div key={row.key} className="stats-table__row" style={{gridTemplateColumns: template}}>
                    {row.cells.map((cell, index) => (
                        <Label key={columns[index]} className={index > 0 ? 'stats-table__number' : 'stats-table__name'}
                               text={cell}/>
                    ))}
                </Div>
            ))}
            {limit != null && rows.length > limit ? (
                <Button variant="secondary" className="stats-table__more"
                        text={expanded ? `Show top ${limit}` : `Show all ${rows.length}`}
                        onClicked={() => setExpanded(!expanded)}/>
            ) : null}
        </Div>
    )
};
