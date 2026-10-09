import {useState, type CSSProperties} from 'react'
import axios from 'axios'
import {createOwner} from '../../../api/setup/owner'
import {Div, Input, Label} from '../../../components/primitives'
import {SettingsRow} from '../../../components/settings-fields.tsx'
import {useAuth} from '../../../context/use-auth.ts'
import {useSettings} from '../../../context/use-settings.ts'
import {useSetup} from '../../../context/use-setup.ts'
import {passwordStrength} from '../../../utils/password-strength.ts'
import {LANGUAGES} from './basics-step.tsx'
import type {StepDef, WizardContext} from '../types.ts'
import {PasswordInput} from '../../../components/password-input.tsx'

interface EarlierAnswers {
    language: string
    timezone: number
}

/**
 * The owner account: its username and password on one step, whose "Next" creates it. The language and timezone
 * answered before the account existed are saved once it does — there was no signed-in user to save them for until
 * now.
 */
export const useAccountStep = ({setBusy}: WizardContext, earlier: EarlierAnswers): { step: StepDef; username: string } => {
    const {refresh} = useSetup()
    const {setSession} = useAuth()
    const {setSettings} = useSettings()

    const [username, setUsername] = useState('')
    const [password, setPassword] = useState('')
    const [repeat, setRepeat] = useState('')
    const [error, setError] = useState<string | null>(null)

    const strength = passwordStrength(password)

    return {
        username: username.trim(),
        step: {
            key: 'account',
            group: 'Account',
            once: true,
            title: 'Account',
            canNext: username.trim().length > 0 && strength.score >= 2 && password.length > 0 && password === repeat,
            onNext: async () => {
                setBusy(true)
                setError(null)
                try {
                    const response = await createOwner(username.trim(), password)
                    // Status first: AuthProvider drops a token while the status still says
                    // "no owner", so the new session may only be adopted once it's caught up.
                    await refresh()
                    setSession(response)
                    await setSettings({language: LANGUAGES[earlier.language], timezone: earlier.timezone})
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
                    <Label className="setup__lead" text="Create the owner account. It can add more users later."/>
                    <Div className="setup__rows">
                        <SettingsRow label="Username">
                            <Input text={username} onChanged={setUsername} placeholder="Choose a username"/>
                        </SettingsRow>
                        <SettingsRow label="Password" help={password ? `Strength: ${strength.label}` : undefined}>
                            <Div className="setup__password">
                                <PasswordInput text={password} onChanged={setPassword} placeholder="Choose a password"/>
                                <Div className="setup__strength">
                                    <Div className="setup__strength-bar" style={{'--score': strength.score} as CSSProperties}/>
                                </Div>
                            </Div>
                        </SettingsRow>
                        <SettingsRow label="Repeat password" accent={repeat.length > 0 && repeat !== password}
                                     help={repeat.length > 0 && repeat !== password ? "Passwords don't match." : undefined}>
                            <PasswordInput text={repeat} onChanged={setRepeat} placeholder="Repeat your password"/>
                        </SettingsRow>
                    </Div>
                    {error ? <Label variant="secondary" className="setup__error" text={error}/> : null}
                </Div>
            ),
        },
    }
};
