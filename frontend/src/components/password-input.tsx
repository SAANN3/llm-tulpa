import {useState} from 'react'
import type {KeyboardEvent} from 'react'
import {Eye, EyeOff} from 'pixelarticons/react'
import '../styles/password-input.scss'
import {Button, Div, Input} from './primitives'

export interface PasswordInputProps {
    text: string
    onChanged: (text: string) => void
    placeholder?: string
    onKeyDown?: (e: KeyboardEvent<HTMLInputElement>) => void
}

/** A password field with a button that shows what was typed, to check it before sending */
export const PasswordInput = ({text, onChanged, placeholder, onKeyDown}: PasswordInputProps) => {
    const [shown, setShown] = useState(false)

    return (
        <Div className="password-input">
            <Input type={shown ? 'text' : 'password'} text={text} onChanged={onChanged} placeholder={placeholder}
                   onKeyDown={onKeyDown}/>
            <Button variant="secondary" className="password-input__toggle" title={shown ? 'Hide' : 'Show'}
                    onClicked={() => setShown((now) => !now)}>
                {shown ? <EyeOff width={16} height={16}/> : <Eye width={16} height={16}/>}
            </Button>
        </Div>
    )
};
