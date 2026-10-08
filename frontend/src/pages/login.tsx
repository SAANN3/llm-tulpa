import {useState} from 'react'
import {Navigate, useLocation, useNavigate} from 'react-router-dom'
import axios from 'axios'

import '../styles/login.scss'
import {Frame} from '../components/frame.tsx'
import {Button, Div, Input, Label} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useAuth} from '../context/use-auth.ts'
import {useSetup} from '../context/use-setup.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {PasswordInput} from '../components/password-input.tsx'

const Login = () => {
    useDocumentTitle('Login')
    const navigate = useNavigate()
    const existingDatabase = Boolean((useLocation().state as { existingDatabase?: boolean } | null)?.existingDatabase)
    const {login, token} = useAuth()
    const {status, loading: setupLoading} = useSetup()
    const [username, setUsername] = useState('')
    const [password, setPassword] = useState('')
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)

    const canSubmit = username.trim().length > 0 && password.length > 0 && !busy

    const onSubmit = async () => {
        if (!canSubmit) return
        setBusy(true)
        setError(null)
        try {
            await login(username.trim(), password)
            navigate('/', {replace: true})
        } catch (e) {
            const status = axios.isAxiosError(e) ? e.response?.status : undefined
            setError(status === 401 ? 'Invalid username or password.' : 'Could not sign in — try again.')
            setBusy(false)
        }
    }

    const onKeyDown = (e: {key: string}) => {
        if (e.key === 'Enter') onSubmit()
    }

    if (setupLoading || !status) return null
    if (!status.configured || !status.has_owner) return <Navigate to="/setup" replace/>
    if (token) return <Navigate to="/" replace/>

    return (
        <Div className="page center vbox login">
            <TypewriterLabel className="login__title" text="[ Login ]" charIntervalMs={30}/>
            <Frame className="login__panel" bodyClassName="login__body" title="Login"
                   actions={[{keys: ['Enter'], shown: 'enter', label: 'sign in'}]}>
                {existingDatabase ? (
                    <Label variant="secondary" className="field__help"
                           text="That database already has an account: sign in with it."/>
                ) : null}
                <Div className="field">
                    <Label className="field__label" text="Username"/>
                    <Input text={username} onChanged={setUsername} placeholder="Enter your username"
                           onKeyDown={onKeyDown}/>
                </Div>
                <Div className="field">
                    <Label className="field__label" text="Password"/>
                    <PasswordInput text={password} onChanged={setPassword} placeholder="Enter your password"
                                   onKeyDown={onKeyDown}/>
                </Div>
                {error ? <Label variant="secondary" className="login__error" text={error}/> : null}
                <Button text={busy ? 'Signing in…' : 'Sign in'} onClicked={onSubmit} disabled={!canSubmit}/>
            </Frame>
        </Div>
    )
};

export default Login
