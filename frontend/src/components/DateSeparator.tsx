import '../styles/DateSeparator.scss'
import { Div, Label } from './primitives'
import { daysBefore } from '../utils/dates'

export interface DateSeparatorProps {
  date: Date
}

/** "Today" / "Yesterday" / a plain date — year included only once it's no longer the
 * current one, so an old chat you scroll back into is still unambiguous. */
function formatDateSeparator(date: Date): string {
  const diffDays = daysBefore(date)
  if (diffDays <= 0) return 'Today'
  if (diffDays === 1) return 'Yesterday'

  const sameYear = date.getFullYear() === new Date().getFullYear()
  return date.toLocaleDateString(undefined, sameYear ? { month: 'long', day: 'numeric' } : { month: 'long', day: 'numeric', year: 'numeric' })
}

/** A centered "Today"/"Yesterday"/date pill breaking up the message list wherever the
 * conversation crosses into a new calendar day — same idea as Telegram's date dividers. */
export function DateSeparator({ date }: DateSeparatorProps) {
  return (
    <Div className="date-separator">
      <Div className="date-separator__chip">
        <Label className="date-separator__label" text={formatDateSeparator(date)} />
      </Div>
    </Div>
  )
}
