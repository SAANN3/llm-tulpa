import { useState } from 'react'

import type { DangerousToolCall, Decisions, PendingConfirmations } from '../hooks/useMessages'
import { ToolAllowance } from '../hooks/useMessages'
import { Button, Div, Label, RadioButton } from './primitives'

interface PendingCallRowProps {
  index: number
  call: DangerousToolCall
  allowance: ToolAllowance
  onChanged: (allowance: ToolAllowance) => void
}

/** One pending tool call's decision row — a radio group when the call offers an escalation to accept, otherwise just its reason with nothing to pick. */
function PendingCallRow({ index, call, allowance, onChanged }: PendingCallRowProps) {
  const groupName = `tool-confirm-${index}`
  const pick = (value: string) => onChanged(value as ToolAllowance)

  return (
    <Div className="vbox panel" variant="secondary" style={{ gap: 6, padding: 12 }}>
      <Label text={call.name} style={{ fontWeight: 600 }} />
      <Label text={JSON.stringify(call.arguments)} style={{ fontSize: 12, opacity: 0.7 }} />
      <Label text={call.reason} style={{ fontSize: 13, opacity: 0.85 }} />
      {call.escalation ? (
        <>
          <Label text={call.escalation.ui_message} style={{ fontSize: 13, opacity: 0.85 }} />
          <Div style={{ display: 'flex', gap: 8, alignItems: 'center' }} variant={allowance === ToolAllowance.Permanent ? 'tertiary' : undefined }>
            <RadioButton name={groupName} value={ToolAllowance.Permanent} checked={allowance === ToolAllowance.Permanent} onChanged={pick} />
            <Label text="Always allow in this chat" />
          </Div>
          <Div style={{ display: 'flex', gap: 8, alignItems: 'center' }} variant={allowance === ToolAllowance.OnlyNow ? 'tertiary' : undefined }>
            <RadioButton name={groupName} value={ToolAllowance.OnlyNow} checked={allowance === ToolAllowance.OnlyNow} onChanged={pick} />
            <Label text="Allow just this once" />
          </Div>
          <Div className="vbox" style={{ gap: 4 }}>
            <Div style={{ display: 'flex', gap: 8, alignItems: 'center' }} variant={allowance === ToolAllowance.Forbid ? 'tertiary' : undefined }>
              <RadioButton name={groupName} value={ToolAllowance.Forbid} checked={allowance === ToolAllowance.Forbid} onChanged={pick} />
              <Label text="Don't allow"/>
            </Div>

          </Div>
        </>
      ) : (
        <Label text="Can't be approved — nothing to grant." style={{ fontSize: 12, opacity: 0.6 }} />
      )}
    </Div>
  )
}

export interface ToolConfirmationProps {
  pending: PendingConfirmations
  onConfirm: (decisions: Decisions) => void
}

/**
 * Shown when a turn pauses on one or more tool calls needing a decision — one row per
 * `pending` entry, defaulting every call to `Forbid` until changed. Purely local state:
 * nothing here talks to the backend, `onConfirm` is handed whatever's been picked and
 * it's the caller's job to act on it.
 */
export function ToolConfirmation({ pending, onConfirm }: ToolConfirmationProps) {
  const [decisions, setDecisions] = useState<Decisions>({})

  const setDecision = (index: number, allowance: ToolAllowance) =>
    setDecisions((prev) => ({ ...prev, [index]: allowance }))

  return (
    // `maxHeight` bounds the whole panel; only the row list scrolls inside it (`minHeight:
    // 0` lets it actually shrink instead of pushing past the bound) — the header and
    // Confirm button stay put outside that scroll area, so however many calls are
    // pending, Confirm never ends up somewhere you have to hunt for.
    <Div className="vbox panel" variant="tertiary" style={{ gap: 10, padding: 14, maxHeight: '45vh' }}>
      <Label text="This turn wants to run tools that need your say-so" style={{ fontWeight: 600 }} />
      <Div className="vbox" style={{ gap: 10, overflowY: 'auto', minHeight: 0 }}>
        {Object.entries(pending).map(([indexStr, call]) => {
          const index = Number(indexStr)
          return (
            <PendingCallRow
              key={index}
              index={index}
              call={call}
              allowance={decisions[index] ?? ToolAllowance.Forbid}
              onChanged={(allowance) => setDecision(index, allowance)}
            />
          )
        })}
      </Div>
      <Button text="Confirm" onClicked={() => onConfirm(decisions)} />
    </Div>
  )
}
