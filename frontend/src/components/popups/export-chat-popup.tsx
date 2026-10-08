import {useState} from 'react'
import '../../styles/export-chat.scss'
import {exportChat} from '../../api/chats/export.ts'
import type {AttachmentMode, ExportFormat} from '../../api/chats/export.ts'
import {saveBlob} from '../../utils/save-blob.ts'
import {Checkbox, Div, Label, Select} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface ExportChatPopupProps {
    open: boolean
    chatId: number
    chatName: string | null
    onClose: () => void
}

const FORMATS: Record<string, ExportFormat> = {PDF: 'pdf', Markdown: 'md'}

const ATTACHMENT_CHOICES: Record<string, AttachmentMode> = {
    'Not included': 'none',
    'In the document': 'embed',
    'In a zip': 'zip',
}

const ATTACHMENT_HINTS: Record<AttachmentMode, string> = {
    none: 'Messages that had attachments say how many were left out',
    embed: 'Images are shown in place and text files printed; other files are left out',
    zip: 'Every file is saved in a .zip next to the document; a PDF shows the images too',
}

/** The chat's name as a file name: what a file system won't take is dropped, the rest (any script) kept */
const fileNameFor = (chatName: string | null, extension: string): string => {
    const name = (chatName ?? '').replace(/[\\/:*?"<>|\r\n]+/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 60)
    return `${name || 'chat'}.${extension}`
}

/** Choose what an exported chat holds, then save it: a PDF or Markdown of the conversation, with
 * its tool calls and its attachments only if asked for (attachments make it a zip). */
export const ExportChatPopup = ({open, chatId, chatName, onClose}: ExportChatPopupProps) => {
    const [format, setFormat] = useState('PDF')
    const [tools, setTools] = useState(false)
    const [attachments, setAttachments] = useState('Not included')
    const [busy, setBusy] = useState(false)
    const [error, setError] = useState<string | null>(null)

    const run = async () => {
        setBusy(true)
        setError(null)
        try {
            const {blob, extension} = await exportChat({chatId, format: FORMATS[format], tools, attachments: ATTACHMENT_CHOICES[attachments]})
            saveBlob(blob, fileNameFor(chatName, extension))
            onClose()
        } catch (e) {
            setError(e instanceof Error ? e.message : "Couldn't export that chat.")
        } finally {
            setBusy(false)
        }
    }

    return (
        <Popup open={open} onClose={busy ? () => undefined : onClose} title="Export chat" width={460} actions={[]}>
            <Div className="export-chat__row">
                <Label text="Format"/>
                <Select values={Object.keys(FORMATS)} selected={format} onChosen={setFormat}/>
            </Div>
            <label className="export-chat__option">
                <Checkbox toggled={tools} onToggled={setTools}/>
                <Div className="vbox">
                    <Label text="Tool calls"/>
                    <Label variant="secondary" className="export-chat__hint"
                           text="The tools the assistant ran, with their arguments and results"/>
                </Div>
            </label>
            <Div className="vbox export-chat__attachments">
                <Div className="export-chat__row">
                    <Label text="Attachments"/>
                    <Select values={Object.keys(ATTACHMENT_CHOICES)} selected={attachments} onChosen={setAttachments}/>
                </Div>
                <Label variant="secondary" className="export-chat__hint"
                       text={ATTACHMENT_HINTS[ATTACHMENT_CHOICES[attachments]]}/>
            </Div>
            {error ? <Label className="export-chat__error" text={error}/> : null}
            <PopupActions
                confirmLabel={busy ? 'Exporting…' : 'Export'}
                confirmDisabled={busy}
                emphasis="confirm"
                onConfirm={run}
                onCancel={onClose}
            />
        </Popup>
    )
};
