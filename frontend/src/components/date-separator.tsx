import '../styles/date-separator.scss'
import {Div, Label} from './primitives'
import {daysBefore} from '../utils/dates'

export interface DateSeparatorProps {
    date: Date
}

/** "Today" / "Yesterday" / a plain date, with the year once it's no longer current */
const formatDateSeparator = (date: Date): string => {
    const diffDays = daysBefore(date)
    if (diffDays <= 0) return 'Today'
    if (diffDays === 1) return 'Yesterday'

    const sameYear = date.getFullYear() === new Date().getFullYear()
    return date.toLocaleDateString(undefined, sameYear ? {month: 'long', day: 'numeric'} : {
        month: 'long',
        day: 'numeric',
        year: 'numeric'
    })
};

/** A centered date pill breaking up the message list at each new calendar day */
export const DateSeparator = ({date}: DateSeparatorProps) => (
    <Div className="date-separator">
        <Div className="date-separator__chip">
            <Label className="date-separator__label" text={formatDateSeparator(date)}/>
        </Div>
    </Div>
);
