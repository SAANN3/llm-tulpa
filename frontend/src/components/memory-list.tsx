import {useCallback, useEffect, useState} from 'react'
import '../styles/memory.scss'
import {clearMemory} from '../api/memory/clear.ts'
import {createMemory} from '../api/memory/create.ts'
import {deleteMemory} from '../api/memory/delete.ts'
import {getMemory} from '../api/memory/list.ts'
import type {MemoryEntry, MemoryEntryRequest} from '../api/memory/types.ts'
import {updateMemory} from '../api/memory/update.ts'
import {errorReason} from '../utils/error-reason.ts'
import {MEMORY_CATEGORIES} from '../utils/memory-categories.ts'
import {Button, Div, Label} from './primitives'
import {ConfirmPopup} from './popups/base/confirm-popup.tsx'
import {MemoryEntryPopup} from './popups/memory-entry-popup.tsx'
import {SettingsRow} from './settings-fields.tsx'

/** What is remembered about the user, by what it is about: each entry with the words it was learned from, to change or delete.
 * Applies at once, unlike the switch and the limits above it, which Save sends. */
export const MemoryList = () => {
    const [entries, setEntries] = useState<MemoryEntry[] | null>(null)
    const [error, setError] = useState<string | null>(null)
    // `null` while closed; an entry to change, or `'new'`
    const [editing, setEditing] = useState<MemoryEntry | 'new' | null>(null)
    const [confirmingClear, setConfirmingClear] = useState(false)

    const reload = useCallback(() => {
        getMemory().then(({entries}) => setEntries(entries)).catch((e) => setError(errorReason(e, 'Could not load the memory.')))
    }, [])
    useEffect(reload, [reload])

    const run = async (action: () => Promise<unknown>) => {
        setError(null)
        try {
            await action()
            reload()
        } catch (e) {
            setError(errorReason(e, 'That did not work.'))
        }
    }

    const submit = (entry: MemoryEntryRequest) =>
        run(() => (editing && editing !== 'new' ? updateMemory({id: editing.id, ...entry}) : createMemory(entry)))

    return (
        <>
            <SettingsRow label="What is remembered" top
                         help="Entries marked as learned come from your own words in a folded chat, and you can see which. The model corrects them when you say something new, and drops the oldest of them for your limit. An entry you add or change is yours: the model never touches it.">
                <Div className="memory-list__buttons">
                    <Button variant="secondary" text="Add" onClicked={() => setEditing('new')}/>
                    <Button variant="secondary" text="Clear all" disabled={!entries?.length} onClicked={() => setConfirmingClear(true)}/>
                </Div>
            </SettingsRow>
            {error ? <Label className="memory-list__error" text={error}/> : null}
            {entries?.length === 0 ? <Label variant="secondary" className="memory-list__empty" text="Nothing is remembered yet."/> : null}
            {MEMORY_CATEGORIES.map(({value, label}) => {
                const group = (entries ?? []).filter((entry) => entry.category === value)
                if (group.length === 0) return null
                return (
                    <Div key={value} className="memory-list__group">
                        <Label className="memory-list__heading" text={label}/>
                        {group.map((entry) => (
                            <Div key={entry.id} className="memory-list__entry">
                                <Div className="memory-list__text">
                                    <Label text={entry.text}/>
                                    <Label variant="secondary" className="memory-list__source"
                                           text={entry.learned && entry.quote ? `learned from: "${entry.quote}"` : 'yours'}/>
                                </Div>
                                <Div className="memory-list__buttons">
                                    <Button variant="secondary" text="Edit" onClicked={() => setEditing(entry)}/>
                                    <Button variant="secondary" text="Delete" onClicked={() => void run(() => deleteMemory(entry.id))}/>
                                </Div>
                            </Div>
                        ))}
                    </Div>
                )
            })}
            <MemoryEntryPopup open={editing !== null} title={editing === 'new' ? 'Add to the memory' : 'Change the entry'}
                              initial={editing && editing !== 'new' ? {category: editing.category, text: editing.text} : undefined}
                              onSubmit={(entry) => void submit(entry)} onClose={() => setEditing(null)}/>
            <ConfirmPopup open={confirmingClear} title="Clear the memory" confirmLabel="Clear all"
                          message="Everything remembered about you is forgotten, the entries you wrote included. Chats that already took it into their prompt keep it until their next fold."
                          onConfirm={() => void run(clearMemory)} onClose={() => setConfirmingClear(false)}/>
        </>
    )
};
