import {useState} from 'react'
import axios from 'axios'
import {setupDatabase} from '../../../api/setup/database'
import {Div, Input, Label} from '../../../components/primitives'
import type {StepDef, WizardContext} from '../types.ts'

/** PostgreSQL connection form; the backend tests it and, on success, persists it and goes live */
export const useDatabaseStep = ({setBusy}: WizardContext): StepDef => {
    const [host, setHost] = useState('localhost')
    const [port, setPort] = useState('5432')
    const [name, setName] = useState('')
    const [user, setUser] = useState('postgres')
    const [password, setPassword] = useState('')
    const [error, setError] = useState<string | null>(null)

    const portValid = /^\d+$/.test(port.trim()) && Number(port) > 0 && Number(port) <= 65535

    return {
        key: 'database',
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
                <Div className="setup__db-grid">
                    <Div className="field">
                        <Label className="field__label" text="Host"/>
                        <Input text={host} onChanged={setHost} placeholder="localhost"/>
                    </Div>
                    <Div className="field">
                        <Label className="field__label" text="Port"/>
                        <Input text={port} onChanged={setPort} placeholder="5432"/>
                    </Div>
                </Div>
                <Div className="field">
                    <Label className="field__label" text="Database name"/>
                    <Input text={name} onChanged={setName} placeholder="tulpa"/>
                </Div>
                <Div className="setup__db-grid">
                    <Div className="field">
                        <Label className="field__label" text="User"/>
                        <Input text={user} onChanged={setUser} placeholder="postgres"/>
                    </Div>
                    <Div className="field">
                        <Label className="field__label" text="Password"/>
                        <Input type="password" text={password} onChanged={setPassword} placeholder="••••••"/>
                    </Div>
                </Div>
                {error ? <Label variant="secondary" className="setup__error" text={error}/> : null}
            </Div>
        ),
    }
};
