import { useState } from 'react'

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
    <Div className="vbox" style={{ gap: 9, padding: '10px 0', borderTop: first ? undefined : '1px solid var(--color-border)' }}>
      <Div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
        <Label variant="secondary" text="PERMISSION NEEDED" style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.08em' }} />
        <Label className="mono" text={call.name} style={{ fontSize: 14 }} />
      </Div>
      <Label variant="secondary" className="mono" text={JSON.stringify(call.arguments)} style={{ fontSize: 12 }} />
      {call.escalation ? (
        <>
          <Label text={call.escalation.ui_message} style={{ fontSize: 15, lineHeight: 1.5, maxWidth: '70ch' }} />
          <Label variant="secondary" text={call.reason} style={{ fontSize: 12 }} />
          <Div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end', flexWrap: 'wrap', marginTop: 2 }}>
            <Button variant="secondary" text="Don't allow" onClicked={() => onDecide(ToolAllowance.Forbid)} />
            <Button variant="secondary" text="Always in this chat" onClicked={() => onDecide(ToolAllowance.Permanent)} />
            <Button variant="primary" text="Allow once" onClicked={() => onDecide(ToolAllowance.OnlyNow)} />
          </Div>
        </>
      ) : (
        <>
          <Label variant="secondary" text="Can't be approved — nothing to grant." style={{ fontSize: 12 }} />
          <Div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 2 }}>
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
    <Div
      className="vbox"
      style={{
        gap: 0,
        padding: '4px 16px 14px',
        maxHeight: '45vh',
        overflowY: 'auto',
        border: '1px solid var(--color-border)',
        borderLeft: '3px solid var(--color-tertiary)',
        borderRadius: 10,
        background: 'var(--color-surface)',
      }}
    >
      {remaining.map(([indexStr, call], i) => {
        const index = Number(indexStr)
        return <PendingCallRow key={index} call={call} first={i === 0} onDecide={(allowance) => decide(index, allowance)} />
      })}
    </Div>
  )
}
