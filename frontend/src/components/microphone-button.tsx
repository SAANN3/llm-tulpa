import type {CSSProperties} from 'react'
import {Mic} from 'pixelarticons/react'
import type {SpeechInputState} from '../hooks/use-speech-input.ts'
import {Button} from './primitives'

export interface MicrophoneButtonProps {
    state: SpeechInputState
    /** How loud it is, 0 to 1, drawn as how strongly the button lights while listening */
    level: number
    /** Voice input can't be used now (no secure page, no model): the button looks dimmed but still answers a press,
     * with what to do about it */
    unavailable: boolean
    onClicked: () => void
}

const TITLES: Record<SpeechInputState, string> = {
    idle: 'Speak instead of typing',
    starting: 'Starting…',
    listening: 'Listening: press to finish',
    finishing: 'Finishing…',
}

/** The microphone button of the message box: press to speak, press again to finish; the text appears in the box as it is
 * understood */
export const MicrophoneButton = ({state, level, unavailable, onClicked}: MicrophoneButtonProps) => (
    <Button
        className={['composer__icon-button', 'composer__mic', state === 'listening' && 'composer__mic--listening', unavailable && 'composer__mic--unavailable'].filter(Boolean).join(' ')}
        style={{'--mic-level': level} as CSSProperties}
        title={unavailable ? 'Voice input is not available: press to see why' : TITLES[state]}
        disabled={state === 'starting' || state === 'finishing'}
        onClicked={onClicked}
    >
        <Mic width={20} height={20}/>
    </Button>
);
