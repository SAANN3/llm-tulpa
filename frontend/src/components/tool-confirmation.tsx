import {useState} from 'react'
import '../styles/tool-confirmation.scss'
import type {AgentScopeGrant, AgentToolCall, Allowance, Decision} from '../api/agent/types.ts'
import {Button, Div, Label} from './primitives'

interface PendingCallRowProps {
    call: AgentToolCall
    escalation: AgentScopeGrant
    reason: string
    first: boolean
    onDecide: (allowance: Allowance) => void
}

/** One pending tool call's decision row */
const PendingCallRow = ({call, escalation, reason, first, onDecide}: PendingCallRowProps) => (
    <Div className={`vbox tool-confirm__row${first ? '' : ' tool-confirm__row--divided'}`}>
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
 * Shown when a run waits for permission. Only the calls the user could grant get a row: a call that
 * is allowed runs, and one that can't be approved is refused for the model, whatever the user says.
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

    return (
        <Div className="vbox tool-confirm">
            {remaining.map(({index, call, escalation, reason}, i) => (
                <PendingCallRow key={index} call={call} escalation={escalation} reason={reason} first={i === 0}
                                onDecide={(allowance) => decide(index, allowance)}/>
            ))}
        </Div>
    )
};
