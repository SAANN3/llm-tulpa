import type {ReactNode} from 'react'
import type {useSettings} from '../../context/use-settings.ts'

/**
 * One wizard step. `body` may be a function of whether the step is the one on screen — every
 * step's body stays mounted (the slides animate sideways), so a step that shouldn't do its
 * work until it's actually reached (the model list fetch) uses that to hold off.
 */
export interface StepDef {
    key: string
    title: string
    body: ReactNode | ((active: boolean) => ReactNode)
    canNext: boolean
    /** Runs when the primary button is pressed; resolving `false` keeps the wizard on this step */
    onNext?: () => Promise<boolean>
    primaryLabel?: string
}

/** What the shell hands every step hook */
export interface WizardContext {
    /** Saves settings for the signed-in user — a no-op until the owner account exists */
    persist: (update: Parameters<ReturnType<typeof useSettings>['setSettings']>[0]) => Promise<void>
    setBusy: (busy: boolean) => void
}
