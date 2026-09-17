import {useState} from 'react'
import '../styles/tool-confirmation.scss'
import type {DangerousToolCall, Decisions, PendingConfirmations} from '../hooks/use-messages.ts'
import {ToolAllowance} from '../hooks/use-messages.ts'
import {Button, Div, Label} from './primitives'

interface PendingCallRowProps {
    call: DangerousToolCall
    first: boolean
    onDecide: (allowance: ToolAllowance) => void
}

/** One pending tool call's decision row, with buttons matching whatever it offers */
const PendingCallRow = ({call, first, onDecide}: PendingCallRowProps) => (
    <Div className={`vbox tool-confirm__row${first ? '' : ' tool-confirm__row--divided'}`}>
        <Div className="tool-confirm__head">
            <Label variant="secondary" className="tool-confirm__badge" text="PERMISSION NEEDED"/>
            <Label className="mono tool-confirm__name" text={call.name}/>
        </Div>
        <Label variant="secondary" className="mono tool-confirm__args" text={JSON.stringify(call.arguments)}/>
        {call.escalation ? (
            <>
                <Label className="tool-confirm__message" text={call.escalation.ui_message}/>
                <Label variant="secondary" className="tool-confirm__reason" text={call.reason}/>
                <Div className="tool-confirm__actions">
                    <Button variant="secondary" text="Don't allow"
                            onClicked={() => onDecide(ToolAllowance.Forbid)}/>
                    <Button variant="secondary" text="Always in this chat"
                            onClicked={() => onDecide(ToolAllowance.Permanent)}/>
                    <Button variant="primary" text="Allow once" onClicked={() => onDecide(ToolAllowance.OnlyNow)}/>
                </Div>
            </>
        ) : (
            <>
                <Label variant="secondary" className="tool-confirm__reason"
                       text="Can't be approved — nothing to grant."/>
                <Div className="tool-confirm__actions">
                    <Button variant="primary" text="OK" onClicked={() => onDecide(ToolAllowance.Forbid)}/>
                </Div>
            </>
        )}
    </Div>
);

export interface ToolConfirmationProps {
    pending: PendingConfirmations
    onConfirm: (decisions: Decisions) => void
}

/** Shown when a turn pauses on tool calls needing a decision */
export const ToolConfirmation = ({pending, onConfirm}: ToolConfirmationProps) => {
    const [decisions, setDecisions] = useState<Decisions>({})

    const decide = (index: number, allowance: ToolAllowance) => {
        const next = {...decisions, [index]: allowance}
        setDecisions(next)
        if (Object.keys(pending).every((i) => next[Number(i)] !== undefined)) onConfirm(next)
    }

    const remaining = Object.entries(pending).filter(([indexStr]) => decisions[Number(indexStr)] === undefined)

    return (
        <Div className="vbox tool-confirm">
            {remaining.map(([indexStr, call], i) => {
                const index = Number(indexStr)
                return <PendingCallRow key={index} call={call} first={i === 0}
                                       onDecide={(allowance) => decide(index, allowance)}/>
            })}
        </Div>
    )
};
