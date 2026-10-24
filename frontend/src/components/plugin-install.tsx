import {useState} from 'react'
import '../styles/plugin-install.scss'
import type {usePluginInstall} from '../hooks/use-plugin-install.ts'
import {DropZone} from './drop-zone.tsx'
import {Button, Div, Input, Label} from './primitives'
import {SettingsSection} from './settings-fields.tsx'

export interface PluginInstallProps {
    install: ReturnType<typeof usePluginInstall>
}

/** The Install tab: a plugin is given as a file or as a git address, and is only looked at: what is in it is shown in a popup
 * (`InstallPluginPopup`) and nothing is installed or run until the owner says to */
export const PluginInstall = ({install}: PluginInstallProps) => {
    const [url, setUrl] = useState('')
    const [gitRef, setGitRef] = useState('')
    const fetch = () => {
        if (url.trim()) void install.stageGit(url.trim(), gitRef.trim())
    }

    return (
        <Div className="plugin-install">
            <Label variant="secondary" className="plugin-install__warning"
                   text="A plugin is a program that runs on this machine with your account's rights. Nothing of it runs until you have seen what is in it and said to install it, so install only plugins you trust."/>
            <SettingsSection title="From a file">
                <DropZone label="Drop a .tulpa file here" accept={['.tulpa', '.zip']} folders={false} multiple={false} busy={install.busy}
                          onFiles={(files) => void install.stageFile(files[0].file)}/>
            </SettingsSection>
            <SettingsSection title="From git">
                <Div className="plugin-install__git">
                    <Input text={url} onChanged={setUrl} placeholder="https://github.com/owner/repo"
                           onKeyDown={(e) => e.key === 'Enter' && fetch()}/>
                    <Div className="plugin-install__git-row">
                        <Input text={gitRef} onChanged={setGitRef} placeholder="branch or tag (optional)"
                               onKeyDown={(e) => e.key === 'Enter' && fetch()}/>
                        <Button variant="secondary" text={install.busy ? 'Fetching…' : 'Fetch'} disabled={install.busy || !url.trim()} onClicked={fetch}/>
                    </Div>
                </Div>
            </SettingsSection>
            {install.error ? <Label className="plugin-install__error" text={install.error}/> : null}
        </Div>
    )
};
