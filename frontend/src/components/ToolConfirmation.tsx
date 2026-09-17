import { useState } from 'react'

import '../styles/ToolConfirmation.scss'
import type { DangerousToolCall, Decisions, PendingConfirmations } from '../hooks/useMessages'
import { ToolAllowance } from '../hooks/useMessages'
import { Button, Div, Label } from './primitives'

interface PendingCallRowProps {
  call: DangerousToolCall
  first: boolean
  onDecide: (allowance: ToolAllowance) => void
}

/** One pending tool call's decision row — three one-click buttons when the call offers
 * an escalation to accept, otherwise just its reason with a single acknowledgment
 * button (there's nothing to grant, so the only possible outcome is `Forbid`). */
function PendingCallRow({ call, first, onDecide }: PendingCallRowProps) {
  return (
    <Div className={`vbox tool-confirm__row${first ? '' : ' tool-confirm__row--divided'}`}>
      <Div className="tool-confirm__head">
        <Label variant="secondary" className="tool-confirm__badge" text="PERMISSION NEEDED" />
        <Label className="mono tool-confirm__name" text={call.name} />
      </Div>
      <Label variant="secondary" className="mono tool-confirm__args" text={JSON.stringify(call.arguments)} />
      {call.escalation ? (
        <>
          <Label className="tool-confirm__message" text={call.escalation.ui_message} />
          <Label variant="secondary" className="tool-confirm__reason" text={call.reason} />
          <Div className="tool-confirm__actions">
            <Button variant="secondary" text="Don't allow" onClicked={() => onDecide(ToolAllowance.Forbid)} />
            <Button variant="secondary" text="Always in this chat" onClicked={() => onDecide(ToolAllowance.Permanent)} />
            <Button variant="primary" text="Allow once" onClicked={() => onDecide(ToolAllowance.OnlyNow)} />
          </Div>
        </>
      ) : (
        <>
          <Label variant="secondary" className="tool-confirm__reason" text="Can't be approved — nothing to grant." />
          <Div className="tool-confirm__actions">
            <Button variant="primary" text="OK" onClicked={() => onDecide(ToolAllowance.Forbid)} />
          </Div>
        </>
      )}
    </Div>
  )
}

export interface ToolConfirmationProps {
  pending: PendingConfirmations
  onConfirm: (decisions: Decisions) => void
}

/**
 * Shown when a turn pauses on one or more tool calls needing a decision. Each row
 * decides on its own click (no shared "Confirm" step): picking an option for a call
 * records it and removes that row from view, and once every pending call has a
 * decision, `onConfirm` fires automatically with everything collected so far. Purely
 * local state until then — nothing here talks to the backend.
 */
export function ToolConfirmation({ pending, onConfirm }: ToolConfirmationProps) {
  const [decisions, setDecisions] = useState<Decisions>({})

  const decide = (index: number, allowance: ToolAllowance) => {
    const next = { ...decisions, [index]: allowance }
    setDecisions(next)
    if (Object.keys(pending).every((i) => next[Number(i)] !== undefined)) onConfirm(next)
  }

  const remaining = Object.entries(pending).filter(([indexStr]) => decisions[Number(indexStr)] === undefined)

  return (
    // `maxHeight` bounds the whole panel and scrolls internally — however many calls
    // are pending, the panel never grows past a reasonable share of the screen.
    <Div className="vbox tool-confirm">
      {remaining.map(([indexStr, call], i) => {
        const index = Number(indexStr)
        return <PendingCallRow key={index} call={call} first={i === 0} onDecide={(allowance) => decide(index, allowance)} />
      })}
    </Div>
  )
}
