import {useRef, useState, type CSSProperties, type ReactNode} from 'react'
import {Navigate, useNavigate} from 'react-router-dom'
import axios from 'axios'
import '../styles/setup.scss'
import {setupDatabase} from '../api/setup/database'
import {createOwner} from '../api/setup/owner'
import {Button, Div, Input, Label, Select, ToggleSwitch} from '../components/primitives'
import {ModelPicker} from '../components/model-picker.tsx'
import {ThemePreview} from '../components/theme-preview.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useAuth} from '../context/use-auth.ts'
import {useSettings} from '../context/use-settings.ts'
import {useSetup} from '../context/use-setup.ts'
import {useTheme} from '../context/use-theme.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {requestNotificationPermission} from '../utils/notifications'
import {validateTimezone} from '../utils/validate-timezone.ts'
import {passwordStrength} from '../utils/password-strength.ts'

const browserTimezoneOffsetHours = (): number => -new Date().getTimezoneOffset() / 60;

const LANGUAGES: Record<string, string> = {English: 'en'}

interface StepDef {
    key: string
    title: string
    body: ReactNode
    canNext: boolean
    onNext?: () => Promise<boolean>
    primaryLabel?: string
    finish?: boolean
}

const Setup = () => {
    useDocumentTitle('Setup')
    const navigate = useNavigate()
    const {status, loading: setupLoading, refresh} = useSetup()
    const {token, setSession} = useAuth()
    const {setSettings} = useSettings()
    const {themeName} = useTheme()

    const needsRef = useRef<{db: boolean; owner: boolean} | null>(null)
    if (status && !needsRef.current) {
        needsRef.current = {db: !status.configured, owner: !status.has_owner}
    }
    const needsDb = needsRef.current?.db ?? true
    const needsOwner = needsRef.current?.owner ?? true

    const [step, setStep] = useState(0)
    const [busy, setBusy] = useState(false)

    const [language, setLanguage] = useState('English')
    const [timezoneText, setTimezoneText] = useState(String(browserTimezoneOffsetHours()))

    const [dbHost, setDbHost] = useState('localhost')
    const [dbPort, setDbPort] = useState('5432')
    const [dbName, setDbName] = useState('')
    const [dbUser, setDbUser] = useState('postgres')
    const [dbPassword, setDbPassword] = useState('')
    const [dbError, setDbError] = useState<string | null>(null)

    const [username, setUsername] = useState('')
    const [password, setPassword] = useState('')
    const [repeat, setRepeat] = useState('')
    const [ownerError, setOwnerError] = useState<string | null>(null)

    const [provider, setProvider] = useState('ollama')
    const [model, setModel] = useState<string | null>(null)
    const [notifications, setNotifications] = useState(false)

    const persist = async (update: Parameters<typeof setSettings>[0]) => {
        if (token) await setSettings(update)
    }

    const tz = validateTimezone(timezoneText)
    const strength = passwordStrength(password)
    const portValid = /^\d+$/.test(dbPort.trim()) && Number(dbPort) > 0 && Number(dbPort) <= 65535

    const steps: StepDef[] = []

    steps.push({
        key: 'welcome',
        title: 'Welcome',
        primaryLabel: 'Start',
        canNext: true,
        onNext: async () => {
            await persist({language: LANGUAGES[language]})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="Let's get your local assistant set up."/>
                <Div className="field">
                    <Label className="field__label" text="Language"/>
                    <Select className="setup__select" values={Object.keys(LANGUAGES)} selected={language}
                            onChosen={setLanguage}/>
                    <Label variant="secondary" className="field__help" text="More languages are coming later."/>
                </Div>
            </Div>
        ),
    })

    steps.push({
        key: 'timezone',
        title: 'Timezone',
        canNext: tz.valid,
        onNext: async () => {
            await persist({timezone: Number(timezoneText)})
            return true
        },
        body: (
            <Div className="setup__step">
                <Div className="field">
                    <Label className="field__label" text="Timezone"/>
                    <Div className="field__control">
                        <Input className="field__input--tz" text={timezoneText} onChanged={setTimezoneText}
                               placeholder="UTC offset"/>
                        {tz.echo ? <Label variant="secondary" text={tz.echo}/> : null}
                    </Div>
                    <Label variant="secondary" className={`field__help${tz.valid ? '' : ' field__help--accent'}`}
                           text={tz.message}/>
                </Div>
            </Div>
        ),
    })

    if (needsDb) {
        steps.push({
            key: 'database',
            title: 'Database',
            canNext: dbHost.trim().length > 0 && dbName.trim().length > 0 && dbUser.trim().length > 0 && portValid,
            onNext: async () => {
                setBusy(true)
                setDbError(null)
                try {
                    await setupDatabase({
                        host: dbHost.trim(),
                        port: Number(dbPort),
                        name: dbName.trim(),
                        user: dbUser.trim(),
                        password: dbPassword,
                    })
                    return true
                } catch (e) {
                    const status = axios.isAxiosError(e) ? e.response?.status : undefined
                    setDbError(status === 400
                        ? 'Could not connect with those details — check them and try again.'
                        : 'Something went wrong reaching the backend.')
                    return false
                } finally {
                    setBusy(false)
                }
            },
            body: (
                <Div className="setup__step">
                    <Label className="setup__lead" text="Connect to your PostgreSQL database."/>
                    <Div className="setup__db-grid">
                        <Div className="field">
                            <Label className="field__label" text="Host"/>
                            <Input text={dbHost} onChanged={setDbHost} placeholder="localhost"/>
                        </Div>
                        <Div className="field">
                            <Label className="field__label" text="Port"/>
                            <Input text={dbPort} onChanged={setDbPort} placeholder="5432"/>
                        </Div>
                    </Div>
                    <Div className="field">
                        <Label className="field__label" text="Database name"/>
                        <Input text={dbName} onChanged={setDbName} placeholder="tulpa"/>
                    </Div>
                    <Div className="setup__db-grid">
                        <Div className="field">
                            <Label className="field__label" text="User"/>
                            <Input text={dbUser} onChanged={setDbUser} placeholder="postgres"/>
                        </Div>
                        <Div className="field">
                            <Label className="field__label" text="Password"/>
                            <Input type="password" text={dbPassword} onChanged={setDbPassword} placeholder="••••••"/>
                        </Div>
                    </Div>
                    {dbError ? <Label variant="secondary" className="setup__error" text={dbError}/> : null}
                </Div>
            ),
        })
    }

    if (needsOwner) {
        steps.push({
            key: 'username',
            title: 'Owner account',
            canNext: username.trim().length > 0,
            body: (
                <Div className="setup__step">
                    <Label className="setup__lead" text="Create the owner account — it can add more users later."/>
                    <Div className="field">
                        <Label className="field__label" text="Username"/>
                        <Input text={username} onChanged={setUsername} placeholder="Choose a username"/>
                    </Div>
                </Div>
            ),
        })

        steps.push({
            key: 'password',
            title: 'Password',
            canNext: strength.score >= 2 && password.length > 0 && password === repeat,
            onNext: async () => {
                setBusy(true)
                setOwnerError(null)
                try {
                    const response = await createOwner(username.trim(), password)
                    // Status first: AuthProvider drops a token while the status still says
                    // "no owner", so the new session may only be adopted once it's caught up.
                    await refresh()
                    setSession(response)
                    await setSettings({language: LANGUAGES[language], timezone: Number(timezoneText)})
                    return true
                } catch (e) {
                    const status = axios.isAxiosError(e) ? e.response?.status : undefined
                    setOwnerError(status === 409 ? 'That username is taken.' : 'Could not create the account.')
                    return false
                } finally {
                    setBusy(false)
                }
            },
            body: (
                <Div className="setup__step">
                    <Div className="field">
                        <Label className="field__label" text="Password"/>
                        <Input type="password" text={password} onChanged={setPassword} placeholder="Choose a password"/>
                        <Div className="setup__strength">
                            <Div className="setup__strength-bar" style={{'--score': strength.score} as CSSProperties}/>
                        </Div>
                        <Label variant="secondary" className="field__help" text={`Strength: ${strength.label}`}/>
                    </Div>
                    <Div className="field">
                        <Label className="field__label" text="Repeat password"/>
                        <Input type="password" text={repeat} onChanged={setRepeat} placeholder="Repeat your password"/>
                        {repeat.length > 0 && repeat !== password ? (
                            <Label variant="secondary" className="field__help field__help--accent"
                                   text="Passwords don't match."/>
                        ) : null}
                    </Div>
                    {ownerError ? <Label variant="secondary" className="setup__error" text={ownerError}/> : null}
                </Div>
            ),
        })
    }

    steps.push({
        key: 'provider',
        title: 'LLM provider',
        canNext: true,
        body: (
            <Div className="setup__step">
                <Div className="field">
                    <Label className="field__label" text="Provider"/>
                    <Select className="setup__select" values={['ollama']} selected={provider} onChosen={setProvider}/>
                    <Label variant="secondary" className="field__help" text="Ollama runs models locally. More providers later."/>
                </Div>
            </Div>
        ),
    })

    const modelStepIndex = steps.length
    steps.push({
        key: 'model',
        title: 'Model',
        canNext: model != null,
        onNext: async () => {
            if (model) await persist({llm_provider: provider, active_model: model})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="Pick the model to chat with — pull one if you don't have it yet."/>
                {step === modelStepIndex ? <ModelPicker selected={model} onSelect={setModel}/> : null}
            </Div>
        ),
    })

    steps.push({
        key: 'theme',
        title: 'Theme',
        canNext: true,
        onNext: async () => {
            await persist({theme: themeName})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="Pick a look."/>
                <ThemePreview/>
            </Div>
        ),
    })

    steps.push({
        key: 'notifications',
        title: 'Notifications',
        canNext: true,
        onNext: async () => {
            await persist({notifications_enabled: notifications})
            return true
        },
        body: (
            <Div className="setup__step">
                <Div className="field">
                    <Div className="field__row">
                        <Label className="field__row-label" text="Notify me when a reply is ready"/>
                        <ToggleSwitch toggled={notifications} onToggled={async (enabled) => {
                            setNotifications(enabled ? await requestNotificationPermission() : false)
                        }}/>
                    </Div>
                    <Label variant="secondary" className="field__help field__help--wide"
                           text="Asks your browser for permission — you can change this later in Settings."/>
                </Div>
            </Div>
        ),
    })

    steps.push({
        key: 'social',
        title: 'All set',
        primaryLabel: 'Finish',
        finish: true,
        canNext: true,
        onNext: async () => {
            navigate('/', {replace: true})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="You're ready to go."/>
                <Label variant="secondary" className="field__help field__help--wide"
                       text="Connect messaging integrations any time from the Plugins page."/>
            </Div>
        ),
    })

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
                                    {s.body}
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
