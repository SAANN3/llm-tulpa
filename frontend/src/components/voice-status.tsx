import '../styles/run-status.scss'
import type {SpeechInputState} from '../hooks/use-speech-input.ts'
import {useSecondsSince} from '../hooks/use-seconds-since.ts'
import {ROW_PX, TuiAreaGraph, TuiCanvas, TuiText} from '../tui-engine'
import {formatDurationShort} from '../utils/format.ts'
import {Button, Div} from './primitives'

/** Height of the graph of the voice level, as the model call's graph in the run status */
const GRAPH_ROWS = 3
/** The title line above it */
const TITLE_ROWS = 1

export interface VoiceStatusProps {
    state: SpeechInputState
    /** How loud each moment was, 0 to 1, oldest first */
    levels: number[]
    startedAt: number | null
    /** Ends the input and keeps what was said, as the microphone button does */
    onStop: () => void
    /** The title says that Escape drops the input: for the one whose page handles that key */
    escCancels?: boolean
    className?: string
}

const TITLES: Record<SpeechInputState, string> = {
    idle: '',
    starting: 'Starting the microphone',
    listening: 'Listening',
    finishing: 'Finishing',
}

/** Where a voice input is while it goes on, in the place the run status takes during a reply: a graph of how loud the
 * microphone is (so a dead or muted one is seen at once) and the stop button */
export function VoiceStatus({state, levels, startedAt, onStop, escCancels = true, className}: VoiceStatusProps) {
    const elapsed = useSecondsSince(startedAt)
    const parts = [TITLES[state], ...(startedAt != null ? [formatDurationShort(elapsed)] : []), ...(state === 'listening' && escCancels ? ['Esc cancels'] : [])]
    // Speech peaks sit far below full scale; the root evens the quiet and the loud out so the graph moves at a normal voice
    const values = levels.map(Math.sqrt)

    return (
        <Div className={['vbox', 'run-status', className].filter(Boolean).join(' ')}>
            <TuiCanvas className="run-activity" style={{height: (TITLE_ROWS + GRAPH_ROWS) * ROW_PX}} label={parts.join(' · ')}>
                <TuiText text={parts.join(' · ')} x={0} y={0} w="100%" truncate/>
                <TuiAreaGraph x={0} y={TITLE_ROWS} w="100%" h={GRAPH_ROWS} values={values} max={1}/>
            </TuiCanvas>
            {state === 'listening' ? <Button className="run-status__stop" variant="secondary" text="Stop" onClicked={onStop}/> : null}
        </Div>
    )
}
