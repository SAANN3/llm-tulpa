import type {ChangeEvent, CSSProperties, DragEvent} from 'react'
import {useEffect, useRef, useState} from 'react'
import type {ThinkChoice} from '../api/agent/types'
import {uploadFile} from '../api/files/upload'
import {getThinkingCapability} from '../api/llm/thinking-capability.ts'
import {Attachment as AttachmentIcon} from 'pixelarticons/react'
import '../styles/user-input.scss'
import {Attachment} from './attachment.tsx'
import {Button, Div, Label, Select, TextField, ToggleSwitch} from './primitives'

export interface UserInputProps {
    text?: string
    blocked: boolean
    className?: string
    onSended: (text: string, think: ThinkChoice, images: string[], fileIds: number[]) => void
    style?: CSSProperties
    placeholder?: string
    clearOnSend?: boolean
    inputDisabled?: boolean
    initialThink?: boolean
    chatId?: number
    /** The chat's bound model — only a change signal, so the thinking options are re-read after a switch */
    model?: string | null
}

const DEFAULT_PLACEHOLDER = 'Message...'

/** Reads a file into raw base64 with no data-URL prefix */
const readFileAsBase64 = (file: File): Promise<string> => new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
        const dataUrl = reader.result as string
        resolve(dataUrl.slice(dataUrl.indexOf(',') + 1))
    }
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(file)
});

const isImageFile = (file: File): boolean => file.type.startsWith('image/');

/** The chat composer: holds draft text and attachments, and sends on Enter or click */
export const UserInput = ({
    text,
    blocked,
    onSended,
    style,
    className,
    placeholder = DEFAULT_PLACEHOLDER,
    clearOnSend = true,
    inputDisabled,
    initialThink = true,
    chatId,
    model,
}: UserInputProps) => {
    const [value, setValue] = useState(text ?? '')
    const [think, setThink] = useState(initialThink)
    const [thinkingModes, setThinkingModes] = useState<string[] | null>(null)
    const [thinkMode, setThinkMode] = useState<string | null>(null)
    const [images, setImages] = useState<string[]>([])
    const [fileIds, setFileIds] = useState<number[]>([])
    const [uploading, setUploading] = useState(false)
    const [draggingOver, setDraggingOver] = useState(false)
    const textareaRef = useRef<HTMLTextAreaElement>(null)
    const fileInputRef = useRef<HTMLInputElement>(null)
    const dragDepthRef = useRef(0)

    useEffect(() => {
        const el = textareaRef.current
        if (!el) return

        el.style.height = 'auto'
        el.style.height = `${el.scrollHeight}px`
    }, [value])

    useEffect(() => {
        let cancelled = false
        getThinkingCapability(chatId).then(
            (capability) => {
                if (cancelled) return
                if (capability.kind === 'graduated') {
                    setThinkingModes(capability.modes)
                    setThinkMode((current) => current ?? capability.modes[0] ?? null)
                } else {
                    setThinkingModes(null)
                }
            },
            () => {
                if (!cancelled) setThinkingModes(null)
            },
        )
        return () => {
            cancelled = true
        }
    }, [chatId, model])

    const canSend = (value.trim().length > 0 || images.length > 0 || fileIds.length > 0) && !uploading

    const send = () => {
        if (blocked || !canSend) return
        const thinkChoice: ThinkChoice = !think ? false : thinkMode ?? true
        onSended(value, thinkChoice, images, fileIds)
        if (clearOnSend) {
            setValue('')
            setImages([])
            setFileIds([])
        }
    }

    const addFiles = async (files: File[]) => {
        const imageFiles = files.filter(isImageFile)
        const otherFiles = files.filter((file) => !isImageFile(file))

        if (imageFiles.length > 0) {
            const encoded = await Promise.all(imageFiles.map(readFileAsBase64))
            setImages((prev) => [...prev, ...encoded])
        }

        if (otherFiles.length > 0) {
            setUploading(true)
            try {
                const uploaded = await Promise.all(otherFiles.map((file) => uploadFile(file, chatId)))
                setFileIds((prev) => [...prev, ...uploaded.map((file) => file.id)])
            } finally {
                setUploading(false)
            }
        }
    }

    const onFilePicked = (e: ChangeEvent<HTMLInputElement>) => {
        const files = Array.from(e.target.files ?? [])
        e.target.value = ''
        void addFiles(files)
    }

    const inputIsDisabled = inputDisabled ?? blocked
    const dropDisabled = inputIsDisabled || uploading
    const isFileDrag = (e: DragEvent<HTMLDivElement>) => e.dataTransfer.types.includes('Files')

    const onDragEnter = (e: DragEvent<HTMLDivElement>) => {
        if (dropDisabled || !isFileDrag(e)) return
        e.preventDefault()
        dragDepthRef.current += 1
        setDraggingOver(true)
    }

    const onDragOver = (e: DragEvent<HTMLDivElement>) => {
        if (dropDisabled || !isFileDrag(e)) return
        e.preventDefault()
    }

    const onDragLeave = (e: DragEvent<HTMLDivElement>) => {
        if (dropDisabled || !isFileDrag(e)) return
        e.preventDefault()
        dragDepthRef.current = Math.max(0, dragDepthRef.current - 1)
        if (dragDepthRef.current === 0) setDraggingOver(false)
    }

    const onDrop = (e: DragEvent<HTMLDivElement>) => {
        if (dropDisabled) return
        e.preventDefault()
        dragDepthRef.current = 0
        setDraggingOver(false)
        const files = Array.from(e.dataTransfer.files)
        if (files.length > 0) void addFiles(files)
    }

    const removeImage = (index: number) => setImages((prev) => prev.filter((_, i) => i !== index))
    const removeFileId = (id: number) => setFileIds((prev) => prev.filter((fileId) => fileId !== id))

    return (
        <Div
            onDragEnter={onDragEnter}
            onDragOver={onDragOver}
            onDragLeave={onDragLeave}
            onDrop={onDrop}
            className={['composer', draggingOver && 'composer--dragging', className].filter(Boolean).join(' ')}
            style={style}
        >
            {draggingOver ? (
                <Div className="vbox center composer__overlay">
                    <Label className="composer__overlay-text" text="Drop to attach"/>
                </Div>
            ) : null}
            {images.length > 0 || fileIds.length > 0 ? (
                <Div className="composer__attachments">
                    {images.map((image, index) => (
                        <Attachment key={`image-${index}`} kind="image" image={image}
                                    onRemove={() => removeImage(index)}/>
                    ))}
                    {fileIds.map((id) => (
                        <Attachment key={`file-${id}`} kind="file" fileId={id} onRemove={() => removeFileId(id)}/>
                    ))}
                </Div>
            ) : null}
            <input ref={fileInputRef} type="file" multiple onChange={onFilePicked} style={{display: 'none'}}/>
            <TextField
                ref={textareaRef}
                className="composer__input"
                text={value}
                onChanged={setValue}
                placeholder={placeholder}
                disabled={inputIsDisabled}
                onKeyDown={(e) => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault()
                        send()
                    }
                }}
            />
            <Div className="composer__footer">
                <Label variant="secondary" className="composer__hint"
                       text="Enter to send · Shift+Enter for a new line"/>
                <Div className="composer__spacer"/>
                <Label variant="secondary" className="composer__think-label" text="Thinking"/>
                <ToggleSwitch toggled={think} onToggled={setThink} disabled={blocked}/>
                {thinkingModes ? (
                    <Div className={['composer__mode', !think && 'composer__mode--disabled'].filter(Boolean).join(' ')}>
                        <Select values={thinkingModes} selected={thinkMode ?? undefined} onChosen={setThinkMode}/>
                    </Div>
                ) : null}
                <Button className="composer__icon-button" onClicked={() => fileInputRef.current?.click()}
                        disabled={dropDisabled}>
                    <AttachmentIcon width={20} height={20}/>
                </Button>
                <Button className="composer__send" text="Send" onClicked={send} disabled={blocked || !canSend}/>
            </Div>
        </Div>
    )
};
