import {useEffect, useState} from 'react'
import type {TabBlock} from '../api/plugins/types.ts'
import {Button, Div, Input} from './primitives'
import {SettingsRow} from './settings-fields.tsx'

interface PluginTabInputProps {
    block: Extract<TabBlock, {type: 'input'}>
    busy: boolean
    act: (action: string, args: Record<string, unknown>) => Promise<void>
}

export const PluginTabInput = ({block, busy, act}: PluginTabInputProps) => {
    const [value, setValue] = useState(block.value ?? '')
    useEffect(() => setValue(block.value ?? ''), [block.value])

    const submit = () => {
        if (!busy) void act(block.action, {...block.args, value})
    }

    return (
        <SettingsRow label={block.label} help={block.help ?? undefined} full>
            <Div className="plugin-tab__input">
                <Input text={value} onChanged={setValue} placeholder={block.placeholder ?? undefined}
                       type={block.password ? 'password' : 'text'} disabled={busy}
                       onKeyDown={(event) => {
                           if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                               event.preventDefault()
                               submit()
                           }
                       }}/>
                <Button text={block.submit_label ?? 'Save'} disabled={busy} onClicked={submit}/>
            </Div>
        </SettingsRow>
    )
};
