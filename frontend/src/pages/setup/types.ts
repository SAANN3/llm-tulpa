import type {ReactNode} from 'react'
import type {useSettings} from '../../context/use-settings.ts'

/** One wizard step. Only the step on screen is rendered; what it holds lives in its hook, so going back and forth keeps
 * it. */
export interface StepDef {
    key: string
    /** The line in the step list it belongs to (Basics, Model…); a step with `part` is one of several under it */
    group: string
    part?: string
    /** The label on the frame's top edge */
    title: string
    body: ReactNode
    canNext: boolean
    /** Runs when the primary button is pressed; resolving `false` keeps the wizard on this step */
    onNext?: () => Promise<boolean>
    primaryLabel?: string
    /** Runs when Back is pressed; returning `true` means the step handled it itself (a sub-view closing) and the wizard stays */
    onBack?: () => boolean
    /** Something is in progress that must not be walked away from: Back and the primary button wait */
    locked?: boolean
    /** What it does can't be done twice (the database is connected, the account created): once it is passed, Back
     * doesn't return to it or to anything before it */
    once?: boolean
}

/** What the shell hands every step hook */
export interface WizardContext {
    /** Saves settings for the signed-in user — a no-op until the owner account exists */
    persist: (update: Parameters<ReturnType<typeof useSettings>['setSettings']>[0]) => Promise<void>
    setBusy: (busy: boolean) => void
}
