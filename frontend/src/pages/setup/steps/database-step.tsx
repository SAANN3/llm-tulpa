import {useState} from 'react'
import axios from 'axios'
import {setupDatabase} from '../../../api/setup/database'
import {getSetupStatus} from '../../../api/setup/status'
import {useSetup} from '../../../context/use-setup.ts'
import {Div, Input, Label} from '../../../components/primitives'
import {SettingsRow} from '../../../components/settings-fields.tsx'
import type {StepDef, WizardContext} from '../types.ts'
import {PasswordInput} from '../../../components/password-input.tsx'

/** PostgreSQL connection form; the backend tests it and, on success, persists it and goes live */
export const useDatabaseStep = ({setBusy}: WizardContext): { step: StepDef; summary: string } => {
    const {refresh} = useSetup()
    const [host, setHost] = useState('localhost')
    const [port, setPort] = useState('5432')
    // What the PostgreSQL that comes with the compose file has, so a clean setup is a click on Next
    const [name, setName] = useState('llm_tulpa')
    const [user, setUser] = useState('postgres')
    const [password, setPassword] = useState('postgres')
    const [error, setError] = useState<string | null>(null)

    const portValid = /^\d+$/.test(port.trim()) && Number(port) > 0 && Number(port) <= 65535

    return {
        summary: `${user.trim()}@${host.trim()}:${port.trim()}/${name.trim()}`,
        step: {
            key: 'database',
            group: 'Database',
            once: true,
            title: 'Database',
            canNext: host.trim().length > 0 && name.trim().length > 0 && user.trim().length > 0 && portValid,
            onNext: async () => {
                setBusy(true)
                setError(null)
                try {
                    await setupDatabase({
                        host: host.trim(),
                        port: Number(port),
                        name: name.trim(),
                        user: user.trim(),
                        password,
                    })
                    // A database that already has an account is somebody's existing install: there is no owner to
                    // create. Refreshing the status makes the wizard's shell send the user to the login page
                    // instead of asking for an account that would be refused.
                    if ((await getSetupStatus()).has_owner) {
                        await refresh()
                        return false
                    }
                    return true
                } catch (e) {
                    const status = axios.isAxiosError(e) ? e.response?.status : undefined
                    setError(status === 400
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
                    <Label variant="secondary" className="field__help"
                           text="These are the details of the database that starts with the app; change them only to use your own."/>
                    <Div className="setup__rows">
                        <SettingsRow label="Server" help="Host and port.">
                            <Div className="setup__db-grid">
                                <Input text={host} onChanged={setHost} placeholder="localhost"/>
                                <Input text={port} onChanged={setPort} placeholder="5432"/>
                            </Div>
                        </SettingsRow>
                        <SettingsRow label="Database name">
                            <Input text={name} onChanged={setName} placeholder="llm_tulpa"/>
                        </SettingsRow>
                        <SettingsRow label="User">
                            <Input text={user} onChanged={setUser} placeholder="postgres"/>
                        </SettingsRow>
                        <SettingsRow label="Password">
                            <PasswordInput text={password} onChanged={setPassword} placeholder="••••••"/>
                        </SettingsRow>
                    </Div>
                    {error ? <Label variant="secondary" className="setup__error" text={error}/> : null}
                </Div>
            ),
        },
    }
};
