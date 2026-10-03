import {useRef, useState, type CSSProperties} from 'react'
import {Navigate, useMatch} from 'react-router-dom'
import '../../styles/setup.scss'
import {AnimatedSize} from '../../components/animated-size.tsx'
import {Button, Div} from '../../components/primitives'
import {TypewriterLabel} from '../../components/typewriter-label.tsx'
import {useAuth} from '../../context/use-auth.ts'
import {useSettings} from '../../context/use-settings.ts'
import {useSetup} from '../../context/use-setup.ts'
import {useDocumentTitle} from '../../hooks/use-document-title.ts'
import {useDatabaseStep} from './steps/database-step.tsx'
import {useFinishStep} from './steps/finish-step.tsx'
import {useModelSteps} from './steps/model-step.tsx'
import {useNotificationsStep} from './steps/notifications-step.tsx'
import {useRebindStep} from './steps/rebind-step.tsx'
import {useOwnerSteps} from './steps/owner-step.tsx'
import {useThemeStep} from './steps/theme-step.tsx'
import {useTimezoneStep} from './steps/timezone-step.tsx'
import {useWelcomeStep} from './steps/welcome-step.tsx'
import type {StepDef, WizardContext} from './types.ts'

// How long a step takes to slide; the same as `.setup__track`'s transition in setup.scss
const SLIDE_MS = 380

/**
 * The first-run wizard. Each step lives in `steps/` as a hook that owns that step's own state
 * and returns its `StepDef`; this shell only decides which steps apply, slides between them
 * and handles Back/Next. Every hook is called on every render (hooks can't be conditional) —
 * the database/owner steps are just left out of the list when that part is already set up.
 */
const Setup = () => {
    useDocumentTitle('Setup')
    const {status, loading: setupLoading} = useSetup()
    const {token} = useAuth()
    const {setSettings} = useSettings()

    // Frozen at the first status we see: finishing the database step flips `configured`, and the
    // steps that were part of this run must not vanish from under the user mid-wizard.
    const needsRef = useRef<{ db: boolean; owner: boolean } | null>(null)
    if (status && !needsRef.current) {
        needsRef.current = {db: !status.configured, owner: !status.has_owner}
    }
    const needsDb = needsRef.current?.db ?? true
    const needsOwner = needsRef.current?.owner ?? true

    const [step, setStep] = useState(0)
    const [busy, setBusy] = useState(false)
    // The step that is sliding out stays rendered until it is gone, instead of emptying as the slide starts
    const [leaving, setLeaving] = useState<number | null>(null)
    const leavingTimer = useRef<number | undefined>(undefined)

    const ctx: WizardContext = {
        persist: async (update) => {
            if (token) await setSettings(update)
        },
        setBusy,
    }

    const welcome = useWelcomeStep(ctx)
    const timezone = useTimezoneStep(ctx)
    const database = useDatabaseStep(ctx)
    const owner = useOwnerSteps(ctx, {language: welcome.language, timezoneText: timezone.timezoneText})
    const model = useModelSteps(ctx)
    const rebind = useRebindStep(model.llamaProfileId)
    // `/setup/update`: an install that is already set up going through what a newer release added
    const upgrade = useMatch('/setup/update') != null
    const theme = useThemeStep(ctx)
    const notifications = useNotificationsStep(ctx)
    const finish = useFinishStep()

    const steps: StepDef[] = upgrade
        ? [...model.steps, ...(model.llamaProfileId != null ? [rebind] : []), finish]
        : [
            welcome.step,
            timezone.step,
            ...(needsDb ? [database] : []),
            ...(needsOwner ? owner : []),
            ...model.steps,
            theme,
            notifications,
            finish,
        ]

    const current = steps[Math.min(step, steps.length - 1)]
    const isFirst = step === 0
    const isLast = step === steps.length - 1

    const goTo = (next: number) => {
        setLeaving(step)
        setStep(next)
        window.clearTimeout(leavingTimer.current)
        leavingTimer.current = window.setTimeout(() => setLeaving(null), SLIDE_MS)
    }

    const onBack = () => {
        if (current.onBack?.()) return
        goTo(Math.max(0, step - 1))
    }
    const onPrimary = async () => {
        if (!current.canNext || busy || current.locked) return
        const advance = current.onNext ? await current.onNext() : true
        if (!advance) return
        if (!isLast) goTo(step + 1)
    }

    if (setupLoading || !status) return null
    // An installation that's already fully set up has nothing to configure without signing in.
    if (status.configured && status.has_owner && !token) {
        // `needsOwner` still true: this run was creating an account, so what it connected to already has one
        return <Navigate to="/login" replace state={{existingDatabase: needsOwner}}/>
    }

    return (
        <Div className="page center vbox setup">
            <TypewriterLabel className="setup__title" text="[ Setup ]" charIntervalMs={30}/>
            <Div className="dos-frame setup__panel">
                <span className="dos-frame__title">{current.title}</span>
                <Div className="dos-frame__body setup__body">
                    <AnimatedSize className="setup__viewport" measure=".setup__slide:not([inert]) > .setup__slide-content" watch={step}>
                        <Div className="setup__track" style={{'--step': step} as CSSProperties}>
                            {steps.map((s, i) => (
                                <div key={s.key} className="setup__slide" inert={i !== step}>
                                    <div className="setup__slide-content">
                                        {typeof s.body === 'function' ? s.body(i === step || i === leaving) : s.body}
                                    </div>
                                </div>
                            ))}
                        </Div>
                    </AnimatedSize>
                    <Div className="center setup__nav">
                        {!isFirst && <Button variant="secondary" text="Back" onClicked={onBack} disabled={current.locked}/>}
                        <Button text={busy ? 'Working…' : (current.primaryLabel ?? 'Next')} onClicked={onPrimary}
                                disabled={!current.canNext || busy || current.locked}/>
                    </Div>
                    <Div className="center setup__dots">
                        {steps.map((s, i) => (
                            <div key={s.key} className={`setup__dot${i === step ? ' setup__dot--active' : ''}`}/>
                        ))}
                    </Div>
                </Div>
            </Div>
        </Div>
    )
};

export default Setup
