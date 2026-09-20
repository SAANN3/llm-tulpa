import {useRef, useState, type CSSProperties} from 'react'
import {Navigate} from 'react-router-dom'
import '../../styles/setup.scss'
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
import {useOwnerSteps} from './steps/owner-step.tsx'
import {useThemeStep} from './steps/theme-step.tsx'
import {useTimezoneStep} from './steps/timezone-step.tsx'
import {useWelcomeStep} from './steps/welcome-step.tsx'
import type {StepDef, WizardContext} from './types.ts'

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
    const theme = useThemeStep(ctx)
    const notifications = useNotificationsStep(ctx)
    const finish = useFinishStep()

    const steps: StepDef[] = [
        welcome.step,
        timezone.step,
        ...(needsDb ? [database] : []),
        ...(needsOwner ? owner : []),
        ...model,
        theme,
        notifications,
        finish,
    ]

    const current = steps[Math.min(step, steps.length - 1)]
    const isFirst = step === 0
    const isLast = step === steps.length - 1

    const onBack = () => setStep((s) => Math.max(0, s - 1))
    const onPrimary = async () => {
        if (!current.canNext || busy) return
        const advance = current.onNext ? await current.onNext() : true
        if (!advance) return
        if (!isLast) setStep((s) => s + 1)
    }

    if (setupLoading || !status) return null
    // An installation that's already fully set up has nothing to configure without signing in.
    if (status.configured && status.has_owner && !token) return <Navigate to="/login" replace/>

    return (
        <Div className="page center vbox setup">
            <TypewriterLabel className="setup__title" text="[ Setup ]" charIntervalMs={30}/>
            <Div className="dos-frame setup__panel">
                <span className="dos-frame__title">{current.title}</span>
                <Div className="dos-frame__body setup__body">
                    <Div className="setup__viewport">
                        <Div className="setup__track" style={{'--step': step} as CSSProperties}>
                            {steps.map((s, i) => (
                                <div key={s.key} className="setup__slide" inert={i !== step}>
                                    {typeof s.body === 'function' ? s.body(i === step) : s.body}
                                </div>
                            ))}
                        </Div>
                    </Div>
                    <Div className="center setup__nav">
                        {!isFirst && <Button variant="secondary" text="Back" onClicked={onBack}/>}
                        <Button text={busy ? 'Working…' : (current.primaryLabel ?? 'Next')} onClicked={onPrimary}
                                disabled={!current.canNext || busy}/>
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
