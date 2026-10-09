import {useState} from 'react'
import '../styles/tool-confirmation.scss'
import type {AgentScopeGrant, AgentToolCall, Allowance, Decision} from '../api/agent/types.ts'
import {Button, Div, Label} from './primitives'

interface PendingCallRowProps {
    call: AgentToolCall
    escalation: AgentScopeGrant
    reason: string
    onDecide: (allowance: Allowance) => void
}

/** One pending tool call's decision row */
const PendingCallRow = ({call, escalation, reason, onDecide}: PendingCallRowProps) => (
    <Div className="vbox tool-confirm__row">
        <Div className="tool-confirm__head">
            <Label variant="secondary" className="tool-confirm__badge" text="PERMISSION NEEDED"/>
            <Label className="mono tool-confirm__name" text={call.name}/>
        </Div>
        <Label variant="secondary" className="mono tool-confirm__args" text={JSON.stringify(call.arguments)}/>
        <Label className="tool-confirm__message" text={escalation.ui_message}/>
        <Label variant="secondary" className="tool-confirm__reason" text={reason}/>
        <Div className="tool-confirm__actions">
            <Button variant="secondary" text="Don't allow" onClicked={() => onDecide('deny')}/>
            <Button variant="secondary" text="Always in this chat" onClicked={() => onDecide('permanent')}/>
            <Button variant="primary" text="Allow once" onClicked={() => onDecide('only_now')}/>
        </Div>
    </Div>
);

export interface ToolConfirmationProps {
    /** The chat's pending tool calls, in order; a decision's `index` is a call's position in this list */
    pending: AgentToolCall[]
    onConfirm: (decisions: Decision[]) => void
}

/**
 * Shown when a run waits for permission. Only the calls the user could grant are asked about: a call that
 * is allowed runs, and one that can't be approved is refused for the model, whatever the user says. Several are
 * asked one after another, with a dot per request in the corner.
 */
export const ToolConfirmation = ({pending, onConfirm}: ToolConfirmationProps) => {
    const [decisions, setDecisions] = useState<Decision[]>([])

    const asking = pending.flatMap((call, index) =>
        call.permission.status === 'denied' && call.permission.escalation
            ? [{index, call, escalation: call.permission.escalation, reason: call.permission.reason}]
            : [])
    const remaining = asking.filter(({index}) => !decisions.some((d) => d.index === index))

    const decide = (index: number, allowance: Allowance) => {
        const next = [...decisions, {index, allowance}]
        setDecisions(next)
        if (asking.every((row) => next.some((d) => d.index === row.index))) onConfirm(next)
    }

    // One request at a time, the next sliding in once this one is answered; the answers still go out together, when
    // the last one is given
    const current = remaining[0]
    if (!current) return null

    return (
        <Div className="vbox tool-confirm">
            <Div key={current.index} className="tool-confirm__slide">
                <PendingCallRow call={current.call} escalation={current.escalation} reason={current.reason}
                                onDecide={(allowance) => decide(current.index, allowance)}/>
            </Div>
            {asking.length > 1 ? (
                <Div className="tool-confirm__dots">
                    {asking.map(({index}) => (
                        <div key={index} className={`tool-confirm__dot${index === current.index ? ' tool-confirm__dot--current' : decisions.some((d) => d.index === index) ? ' tool-confirm__dot--done' : ''}`}/>
                    ))}
                </Div>
            ) : null}
        </Div>
    )
};
