export interface Settings {
  name: string
  /** UTC offset in whole hours (e.g. `-5`, `9`), not an IANA timezone name. */
  timezone: number
  /** Whether to ask the browser to show a notification when an assistant reply finishes. */
  notifications_enabled: boolean
}
