import {useState} from 'react'
import '../styles/tool-choice.scss'
import type {AvailableTool} from '../api/chats/available-tools.ts'
import type {ToolMode} from '../api/settings/types.ts'
import {Button, ChoiceGroup, Div, Label, ToggleSwitch} from './primitives'

interface ToolChoiceProps {
    catalog: AvailableTool[]
    mode: ToolMode
    /** The names the mode lists */
    names: string[]
    onMode: (mode: ToolMode) => void
    onNames: (names: string[]) => void
    disabled?: boolean
}

const sent = (mode: ToolMode, listed: Set<string>, name: string): boolean =>
    mode === 'whitelist' ? listed.has(name) : !listed.has(name)

/** `on` is whether the tool is sent. A blacklist lists the ones that are not. */
const withTool = (mode: ToolMode, names: string[], name: string, on: boolean): string[] => {
    const rest = names.filter((listed) => listed !== name)
    const listed = mode === 'whitelist' ? on : !on
    return listed ? [...rest, name].sort() : rest
}

const withDomain = (mode: ToolMode, names: string[], domainNames: string[], on: boolean): string[] => {
    const drop = new Set(domainNames)
    const rest = names.filter((listed) => !drop.has(listed))
    const listed = mode === 'whitelist' ? on : !on
    return listed ? [...rest, ...domainNames].sort() : rest
}

const domainsOf = (catalog: AvailableTool[]): [string, AvailableTool[]][] => {
    const groups = new Map<string, AvailableTool[]>()
    for (const tool of catalog) {
        const domain = tool.name.split('.')[0] ?? tool.name
        groups.set(domain, [...(groups.get(domain) ?? []), tool])
    }
    return [...groups.entries()]
}

/** A tool selection as domains of switches. A tool is on when the mode sends it. A domain switch is on when any
 * tool in it is on: turning it off clears the domain, turning it on from all-off sends every tool in it. */
export const ToolChoice = ({catalog, mode, names, onMode, onNames, disabled}: ToolChoiceProps) => {
    const listed = new Set(names)
    const domains = domainsOf(catalog)
    const [collapsed, setCollapsed] = useState<Set<string>>(new Set())
    const fold = (domain: string) => setCollapsed((now) => {
        const next = new Set(now)
        if (next.has(domain)) next.delete(domain)
        else next.add(domain)
        return next
    })

    return (
        <Div className="vbox tool-choice">
            <Div className="tool-choice__bar">
                <ChoiceGroup label="Tool mode" chosen={mode} onChosen={(value) => {
                    if (value === mode) return
                    // An empty list is every tool for a blacklist and none for a whitelist
                    onMode(value as ToolMode)
                    onNames([])
                }} options={[
                    {value: 'blacklist', label: 'Blacklist', help: 'Every tool is sent, except the ones switched off'},
                    {value: 'whitelist', label: 'Whitelist', help: 'Only the switched-on tools are sent. None on means no tools'},
                ]}/>
                <Div className="tool-choice__folds">
                    <Button variant="secondary" text="Expand all" disabled={disabled || collapsed.size === 0}
                            onClicked={() => setCollapsed(new Set())}/>
                    <Button variant="secondary" text="Collapse all" disabled={disabled || collapsed.size === domains.length}
                            onClicked={() => setCollapsed(new Set(domains.map(([domain]) => domain)))}/>
                </Div>
            </Div>
            <Div className="vbox tool-choice__list">
                {domains.map(([domain, tools]) => {
                    const anyOn = tools.some((tool) => sent(mode, listed, tool.name))
                    const open = !collapsed.has(domain)
                    return (
                        <Div key={domain} className="vbox tool-choice__domain">
                            <Div className="tool-choice__domain-row">
                                <Label className="tool-choice__domain-name" text={domain}/>
                                <Div className="tool-choice__domain-controls">
                                    <ToggleSwitch toggled={anyOn} disabled={disabled}
                                                  onToggled={(on) => onNames(withDomain(mode, names, tools.map((tool) => tool.name), on))}/>
                                    <Button variant="secondary" className="tool-choice__fold" text={open ? 'Hide' : 'Show'}
                                            disabled={disabled} onClicked={() => fold(domain)}/>
                                </Div>
                            </Div>
                            {open ? tools.map((tool) => (
                                <Div key={tool.name} className="tool-choice__tool">
                                    <Div className="vbox tool-choice__tool-main">
                                        <Label text={tool.name.slice(domain.length + 1)}/>
                                        <Label variant="secondary" className="tool-choice__desc" text={tool.description}/>
                                    </Div>
                                    <ToggleSwitch toggled={sent(mode, listed, tool.name)} disabled={disabled}
                                                  onToggled={(on) => onNames(withTool(mode, names, tool.name, on))}/>
                                </Div>
                            )) : null}
                        </Div>
                    )
                })}
            </Div>
        </Div>
    )
}
