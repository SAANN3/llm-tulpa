import {useState, type CSSProperties} from 'react'
import axios from 'axios'
import {createOwner} from '../../../api/setup/owner'
import {Div, Input, Label} from '../../../components/primitives'
import {useAuth} from '../../../context/use-auth.ts'
import {useSettings} from '../../../context/use-settings.ts'
import {useSetup} from '../../../context/use-setup.ts'
import {passwordStrength} from '../../../utils/password-strength.ts'
import {LANGUAGES} from './welcome-step.tsx'
import type {StepDef, WizardContext} from '../types.ts'

interface EarlierAnswers {
    language: string
    timezoneText: string
}

/**
 * The owner account: a username step, then a password step whose "Next" creates the account.
 * The language and timezone answered before the account existed are saved once it does — there
 * was no signed-in user to save them for until now.
 */
export const useOwnerSteps = ({setBusy}: WizardContext, earlier: EarlierAnswers): StepDef[] => {
    const {refresh} = useSetup()
    const {setSession} = useAuth()
    const {setSettings} = useSettings()

    const [username, setUsername] = useState('')
    const [password, setPassword] = useState('')
    const [repeat, setRepeat] = useState('')
    const [error, setError] = useState<string | null>(null)

    const strength = passwordStrength(password)

    const usernameStep: StepDef = {
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
    }

    const passwordStep: StepDef = {
        key: 'password',
        title: 'Password',
        canNext: strength.score >= 2 && password.length > 0 && password === repeat,
        onNext: async () => {
            setBusy(true)
            setError(null)
            try {
                const response = await createOwner(username.trim(), password)
                // Status first: AuthProvider drops a token while the status still says
                // "no owner", so the new session may only be adopted once it's caught up.
                await refresh()
                setSession(response)
                await setSettings({language: LANGUAGES[earlier.language], timezone: Number(earlier.timezoneText)})
                return true
            } catch (e) {
                const status = axios.isAxiosError(e) ? e.response?.status : undefined
                setError(status === 409 ? 'That username is taken.' : 'Could not create the account.')
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
                {error ? <Label variant="secondary" className="setup__error" text={error}/> : null}
            </Div>
        ),
    }

    return [usernameStep, passwordStep]
};
