import {Fragment, useRef, useState, type ReactNode} from 'react'
import {Navigate, useMatch} from 'react-router-dom'
import {Brush, Cpu, Database, Flag, Message, SlidersHorizontal, User} from 'pixelarticons/react'
import '../../styles/setup.scss'
import {findBackground} from '../../backgrounds'
import {Frame} from '../../components/frame.tsx'
import {Button, Div} from '../../components/primitives'
import {TypewriterLabel} from '../../components/typewriter-label.tsx'
import {useAuth} from '../../context/use-auth.ts'
import {useSettings} from '../../context/use-settings.ts'
import {useSetup} from '../../context/use-setup.ts'
import {useTheme} from '../../context/use-theme.ts'
import {useDocumentTitle} from '../../hooks/use-document-title.ts'
import {themeDisplayNames} from '../../themes'
import {useAccountStep} from './steps/account-step.tsx'
import {useBasicsStep} from './steps/basics-step.tsx'
import {useDatabaseStep} from './steps/database-step.tsx'
import {useFinishStep} from './steps/finish-step.tsx'
import {useLookStep} from './steps/look-step.tsx'
import {useModelSteps} from './steps/model-step.tsx'
import {useRebindStep} from './steps/rebind-step.tsx'
import type {StepDef, WizardContext} from './types.ts'

const ICON_SIZE = 16
const ICONS: Record<string, ReactNode> = {
    Basics: <SlidersHorizontal width={ICON_SIZE} height={ICON_SIZE}/>,
    Database: <Database width={ICON_SIZE} height={ICON_SIZE}/>,
    Account: <User width={ICON_SIZE} height={ICON_SIZE}/>,
    Model: <Cpu width={ICON_SIZE} height={ICON_SIZE}/>,
    Look: <Brush width={ICON_SIZE} height={ICON_SIZE}/>,
    'Existing chats': <Message width={ICON_SIZE} height={ICON_SIZE}/>,
    'All set': <Flag width={ICON_SIZE} height={ICON_SIZE}/>,
}

type Progress = 'done' | 'current' | 'ahead'

/** A step's mark in the list: its icon until it is reached, then `>` while it is the current one and `✓` once done.
 * A part (under Model) has no icon, so it shows a dot while ahead. */
const Mark = ({progress, icon}: { progress: Progress; icon?: ReactNode }) => (
    <span className="setup__mark">{progress === 'done' ? '✓' : progress === 'current' ? '>' : icon ?? '·'}</span>
)

/** The step list on the left: a line per group, the parts of a group with several steps indented under it */
const StepList = ({steps, current}: { steps: StepDef[]; current: number }) => {
    const groups = steps.reduce<{ name: string; first: number; last: number; parts: { name: string; index: number }[] }[]>((list, step, index) => {
        const group = list[list.length - 1]?.name === step.group ? list[list.length - 1] : null
        if (group) {
            group.last = index
            if (step.part) group.parts.push({name: step.part, index})
            return list
        }
        return [...list, {name: step.group, first: index, last: index, parts: step.part ? [{name: step.part, index}] : []}]
    }, [])
    const progressOf = (first: number, last: number): Progress => (current > last ? 'done' : current >= first ? 'current' : 'ahead')

    return (
        <Div className="setup__rail">
            {groups.map((group) => (
                <Fragment key={group.name}>
                    <Div className={`setup__rail-item setup__rail-item--${progressOf(group.first, group.last)}`}>
                        <Mark progress={progressOf(group.first, group.last)} icon={ICONS[group.name]}/>
                        <span>{group.name}</span>
                    </Div>
                    {group.parts.map((part) => (
                        <Div key={part.name} className={`setup__rail-item setup__rail-item--part setup__rail-item--${progressOf(part.index, part.index)}`}>
                            <Mark progress={progressOf(part.index, part.index)}/>
                            <span>{part.name}</span>
                        </Div>
                    ))}
                </Fragment>
            ))}
        </Div>
    )
}

/**
 * The first-run wizard. Each step lives in `steps/` as a hook that owns that step's own state
 * and returns its `StepDef`; this shell only decides which steps apply, shows the one on screen
 * with the list of all of them, and handles Back/Next. Every hook is called on every render (hooks
 * can't be conditional) — the database/account steps are just left out of the list when that part
 * is already set up.
 */
const Setup = () => {
    useDocumentTitle('Setup')
    const {status, loading: setupLoading} = useSetup()
    const {token} = useAuth()
    const {setSettings} = useSettings()
    const {themeName, background} = useTheme()

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

    const basics = useBasicsStep(ctx)
    const database = useDatabaseStep(ctx)
    const account = useAccountStep(ctx, {language: basics.language, timezone: basics.timezone})
    const model = useModelSteps(ctx)
    const rebind = useRebindStep(model.llamaProfileId)
    // `/setup/update`: an install that is already set up going through what a newer release added
    const upgrade = useMatch('/setup/update') != null
    const look = useLookStep(ctx)
    const summary: [string, string][] = [
        ...(!upgrade && needsOwner && account.username ? [['Account', `${account.username} (owner)`] as [string, string]] : []),
        ...(!upgrade && needsDb ? [['Database', database.summary] as [string, string]] : []),
        ...(model.summary ? [['Model', model.summary] as [string, string]] : []),
        ...(!upgrade ? [['Look', `${themeDisplayNames[themeName]} · ${findBackground(background.id)?.name ?? (background.id === 'dots' ? 'Dots' : 'Off')}`] as [string, string]] : []),
    ]
    const finish = useFinishStep(ctx, {summary, withNotifications: !upgrade})

    const steps: StepDef[] = upgrade
        ? [...model.steps, ...(model.llamaProfileId != null ? [rebind] : []), finish]
        : [
            basics.step,
            ...(needsDb ? [database.step] : []),
            ...(needsOwner ? [account.step] : []),
            ...model.steps,
            look,
            finish,
        ]

    const current = steps[Math.min(step, steps.length - 1)]
    const isLast = step === steps.length - 1
    // Back goes no further than the step after one that can't be done twice
    const canBack = step > 0 && !steps[step - 1].once && !current.locked

    const onBack = () => {
        if (current.onBack?.()) return
        setStep(Math.max(0, step - 1))
    }
    const onPrimary = async () => {
        if (!current.canNext || busy || current.locked) return
        const advance = current.onNext ? await current.onNext() : true
        if (!advance) return
        if (!isLast) setStep(step + 1)
    }

    if (setupLoading || !status) return null
    // An installation that's already fully set up has nothing to configure without signing in.
    if (status.configured && status.has_owner && !token) {
        // `needsOwner` still true: this run was creating an account, so what it connected to already has one
        return <Navigate to="/login" replace state={{existingDatabase: needsOwner}}/>
    }

    const primaryLabel = current.primaryLabel ?? 'Next'
    return (
        <Div className="page center vbox setup">
            <TypewriterLabel className="setup__title" text="[ Setup ]" charIntervalMs={30}/>
            <Frame className="setup__panel" bodyClassName="setup__body" title={current.title}
                   actions={[{keys: ['Enter'], shown: 'enter', label: primaryLabel.toLowerCase(), run: () => void onPrimary()}]}
                   onEscape={canBack ? onBack : undefined} escapeLabel="back"
                   footerStart={canBack ? <Button variant="secondary" text="Back" onClicked={onBack}/> : <span/>}
                   footerEnd={<Button text={busy ? 'Working…' : primaryLabel} onClicked={onPrimary}
                                      disabled={!current.canNext || busy || current.locked}/>}>
                <StepList steps={steps} current={step}/>
                <Div key={current.key} className="setup__content">
                    {current.body}
                </Div>
            </Frame>
        </Div>
    )
};

export default Setup
