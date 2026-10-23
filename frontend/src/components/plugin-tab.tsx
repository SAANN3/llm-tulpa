import {useRef, useState} from 'react'
import '../styles/plugin-tab.scss'
import type {TabBlock, TabRow} from '../api/plugins/types.ts'
import {usePluginTabLayout} from '../hooks/use-plugin-tab-layout.ts'
import {Button, Div, Label, ToggleSwitch} from './primitives'
import {ConfirmPopup} from './popups/base/confirm-popup.tsx'
import {Popup} from './popups/base/popup.tsx'
import {DropZone} from './drop-zone.tsx'
import {SettingsRow, SettingsSection} from './settings-fields.tsx'

export interface PluginTabProps {
    pluginName: string
    pluginSubname: string
}

/** What a plugin's own Settings tab holds, drawn from the layout the plugin sends (see `plugins::tab` in the backend):
 * sections of text, switches, lists of rows with buttons, and drop zones. What it shows and what the user does in it are
 * `usePluginTabLayout`'s; this only draws. */
export const PluginTab = ({pluginName, pluginSubname}: PluginTabProps) => {
    const {layout, error, busy, popup, closePopup, act, upload} = usePluginTabLayout(pluginName, pluginSubname)
    // What waits for a yes: the button and its row, with the question
    const [asking, setAsking] = useState<{ action: string; row: string; question: string } | null>(null)
    // The button whose picker is open: one hidden input serves every row's upload button
    const picker = useRef<HTMLInputElement>(null)
    const picking = useRef<{ action: string; row: string } | null>(null)

    // The key has the place too: a plugin that gives two rows one id must not make the list unstable
    const drawRow = (row: TabRow, at: number) => (
        <Div key={`${at}:${row.id}`} className="plugin-tab__row">
            <Div className="plugin-tab__row-text">
                <Div className="plugin-tab__row-title">
                    <Label text={row.title}/>
                    {row.badge ? <Label variant="secondary" className="plugin-tab__badge" text={row.badge}/> : null}
                </Div>
                {row.subtitle ? <Label variant="secondary" className="plugin-tab__subtitle" text={row.subtitle}/> : null}
            </Div>
            <Div className="plugin-tab__row-controls">
                {row.switch ? (
                    <ToggleSwitch toggled={row.switch.on} disabled={busy}
                                  onToggled={(on) => void act(row.switch!.action, {row: row.id, on})}/>
                ) : null}
                {row.buttons.map((button) => (
                    <Button key={button.action} variant="secondary" text={button.label} disabled={busy}
                            onClicked={() => {
                                if (button.upload) {
                                    picking.current = {action: button.action, row: row.id}
                                    picker.current?.click()
                                } else if (button.confirm) setAsking({action: button.action, row: row.id, question: button.confirm})
                                else void act(button.action, {row: row.id})
                            }}/>
                ))}
            </Div>
        </Div>
    )

    const drawBlock = (block: TabBlock, at: number) => {
        switch (block.type) {
            case 'text':
                return <Label key={at} variant={block.dim ? 'secondary' : undefined} className="plugin-tab__text" text={block.text}/>
            case 'switch':
                return (
                    <SettingsRow key={at} label={block.label} help={block.help ?? undefined}>
                        <ToggleSwitch toggled={block.on} disabled={busy} onToggled={(on) => void act(block.action, {on})}/>
                    </SettingsRow>
                )
            case 'items':
                return (
                    <Div key={at} className="plugin-tab__rows">
                        {block.rows.length === 0 ? <Label variant="secondary" text={block.empty}/> : block.rows.map((row, at) => drawRow(row, at))}
                    </Div>
                )
            case 'drop':
                return (
                    <Div key={at} className="plugin-tab__drop">
                        <DropZone label={block.label} accept={block.accept} busy={busy} onFiles={(files) => void upload(block.action, files)}/>
                        {block.help ? <Label variant="secondary" className="plugin-tab__text" text={block.help}/> : null}
                    </Div>
                )
        }
    }

    if (!layout) return error ? <Label className="plugin-tab__error" text={error}/> : null

    return (
        <>
            {layout.sections.map((section, at) => {
                const blocks = section.blocks.map(drawBlock)
                return section.title
                    ? <SettingsSection key={at} title={section.title}>{blocks}</SettingsSection>
                    : <Div key={at} className="settings-section">{blocks}</Div>
            })}
            {error ? <Label className="plugin-tab__error" text={error}/> : layout.message ? <Label variant="secondary" className="plugin-tab__message" text={layout.message}/> : null}
            <input ref={picker} type="file" multiple hidden
                   onChange={(e) => {
                       const files = Array.from(e.target.files ?? []).map((file) => ({path: file.name, file}))
                       e.target.value = ''
                       if (files.length > 0 && picking.current) void upload(picking.current.action, files, picking.current.row)
                   }}/>
            <ConfirmPopup open={asking !== null} title="Are you sure?" confirmLabel="Yes" message={asking?.question ?? ''}
                          onConfirm={() => asking && void act(asking.action, {row: asking.row})} onClose={() => setAsking(null)}/>
            <Popup open={popup !== null} title={popup?.title ?? ''} width={760} actions={[]} onClose={closePopup}>
                <pre className="plugin-tab__popup-text">{popup?.text}</pre>
            </Popup>
        </>
    )
};
