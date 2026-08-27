/** Midnight of `date`'s calendar day, as a timestamp — the basis for "is this the same
 * day as X" comparisons that shouldn't drift with time-of-day (e.g. two events an hour
 * apart either side of midnight are different days; two events 20 hours apart on the
 * same real day are not). */
function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
}

/** How many calendar days `date` is before `from` (0 = same day, 1 = the day before, …).
 * Negative if `date` is actually after `from`. */
export function daysBefore(date: Date, from: Date = new Date()): number {
  return Math.round((startOfDay(from) - startOfDay(date)) / 86_400_000)
}

/** Whether `a` and `b` fall on the same calendar day. */
export function isSameDay(a: Date, b: Date): boolean {
  return startOfDay(a) === startOfDay(b)
}
