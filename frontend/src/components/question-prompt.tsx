import {useState} from 'react'
import '../styles/tool-confirmation.scss'
import '../styles/question-prompt.scss'
import {useKeyLayer} from '../hooks/use-key-layer.ts'
import {Button, Div, Input, Label} from './primitives'

const CUSTOM_ID = 'custom'

export interface OpenQuestion {
    key: number
    args: Record<string, unknown>
}

const optionsOf = (args: Record<string, unknown>): {id: string, label: string}[] => {
    const raw = args.options
    if (!Array.isArray(raw)) return []
    return raw.flatMap((item) => {
        if (item == null || typeof item !== 'object') return []
        const id = 'id' in item ? String((item as {id: unknown}).id) : ''
        const label = 'label' in item ? String((item as {label: unknown}).label) : ''
        if (!id || !label || id === CUSTOM_ID) return []
        return [{id, label}]
    })
}

interface RowProps {
    args: Record<string, unknown>
    onPicked: (text: string) => void
}

const QuestionRow = ({args, onPicked}: RowProps) => {
    const options = optionsOf(args)
    const question = typeof args.question === 'string' ? args.question : ''
    const allowCustom = args.allow_custom !== false
    const [custom, setCustom] = useState('')
    const [customOpen, setCustomOpen] = useState(false)

    useKeyLayer(true, (e) => {
        if (e.key >= '1' && e.key <= '9') {
            const option = options[Number(e.key) - 1]
            if (option) {
                onPicked(option.label)
                return true
            }
        }
        return false
    })

    return (
        <Div className="vbox tool-confirm__row">
            <Div className="tool-confirm__head">
                <Label variant="secondary" className="tool-confirm__badge" text="QUESTION"/>
            </Div>
            <Label className="tool-confirm__message" text={question}/>
            <Div className="vbox question-prompt__options">
                {options.map((option, i) => (
                    <Button key={option.id} variant="secondary" className="question-prompt__option"
                            text={`${i + 1}. ${option.label}`} onClicked={() => onPicked(option.label)}/>
                ))}
                {allowCustom ? (
                    customOpen ? (
                        <Div className="question-prompt__custom">
                            <Input className="question-prompt__input" text={custom} placeholder="Your answer"
                                   autoFocus onChanged={setCustom}
                                   onKeyDown={(e) => {
                                       if (e.key === 'Enter' && custom.trim()) onPicked(custom.trim())
                                   }}/>
                            <Button variant="primary" text="Send" disabled={!custom.trim()}
                                    onClicked={() => onPicked(custom.trim())}/>
                        </Div>
                    ) : (
                        <Button variant="secondary" className="question-prompt__option" text="Something else"
                                onClicked={() => setCustomOpen(true)}/>
                    )
                ) : null}
            </Div>
        </Div>
    )
}

export interface QuestionPromptProps {
    questions: OpenQuestion[]
    /** One message, once every question in this batch has a pick. */
    onAnswer: (text: string) => void
}

/**
 * The questions the model asked since the user's last message, above the composer. One at a time,
 * the same way a permission prompt walks a batch; the picks go out together as the next message.
 */
export const QuestionPrompt = ({questions, onAnswer}: QuestionPromptProps) => {
    const [picks, setPicks] = useState<{key: number, text: string}[]>([])
    const remaining = questions.filter((question) => !picks.some((pick) => pick.key === question.key))
    const current = remaining[0]
    if (!current) return null

    const pick = (text: string) => {
        const next = [...picks, {key: current.key, text}]
        setPicks(next)
        if (!questions.every((question) => next.some((item) => item.key === question.key))) return
        const lines = questions.map((question) => {
            const chosen = next.find((item) => item.key === question.key)?.text ?? ''
            const title = typeof question.args.question === 'string' ? question.args.question : ''
            return questions.length === 1 ? chosen : `${title}: ${chosen}`
        })
        onAnswer(lines.join('\n'))
    }

    return (
        <Div className="vbox tool-confirm">
            <Div key={current.key} className="tool-confirm__slide">
                <QuestionRow args={current.args} onPicked={pick}/>
            </Div>
            {questions.length > 1 ? (
                <Div className="tool-confirm__dots">
                    {questions.map((question) => (
                        <div key={question.key} className={`tool-confirm__dot${question.key === current.key ? ' tool-confirm__dot--current' : picks.some((item) => item.key === question.key) ? ' tool-confirm__dot--done' : ''}`}/>
                    ))}
                </Div>
            ) : null}
        </Div>
    )
}
