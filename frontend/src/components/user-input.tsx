import type {ChangeEvent, CSSProperties, DragEvent} from 'react'
import {useEffect, useRef, useState} from 'react'
import type {ThinkChoice} from '../api/agent/types'
import {uploadFile} from '../api/files/upload'
import {getThinkingCapability, type ThinkingModel} from '../api/llm/thinking-capability.ts'
import {Attachment as AttachmentIcon} from 'pixelarticons/react'
import '../styles/user-input.scss'
import {Attachment} from './attachment.tsx'
import {ModelMenu, type ThinkingLevels} from './model-menu.tsx'
import {Button, Div, Label, TextField} from './primitives'
import type {ModelChoice} from '../utils/model-choice.ts'

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
    /** A message being edited: its text and attachments fill the composer until it is sent or cancelled. `key` tells one edit from the next. */
    editing?: { key: number; text: string; images: string[]; fileIds: number[] } | null
    onCancelEdit?: () => void
    /** The model the message goes to, shown on the model button; the thinking options are re-read when it changes */
    modelChoice?: ModelChoice | null
    onModelPicked?: (choice: ModelChoice) => void
    /** For a new chat's model that isn't the user's default: marks the button and offers going back to the default */
    onUseDefault?: () => void
    /** The user's default model, marked in the model menu where a pick is only for a new chat */
    defaultChoice?: ModelChoice | null
    /** For a chat that doesn't exist yet (the home page): the model picked for it, whose thinking options are offered instead of the default model's */
    startModel?: ThinkingModel | null
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
    modelChoice,
    onModelPicked,
    onUseDefault,
    defaultChoice,
    startModel,
    editing,
    onCancelEdit,
}: UserInputProps) => {
    const [value, setValue] = useState(text ?? '')
    const [think, setThink] = useState(initialThink)
    const [thinkingModes, setThinkingModes] = useState<string[] | null>(null)
    // Until the model is asked, it is taken to think: the switch has always been on by default
    const [canThink, setCanThink] = useState(true)
    const [thinkMode, setThinkMode] = useState<string | null>(null)
    const [images, setImages] = useState<string[]>([])
    const [fileIds, setFileIds] = useState<number[]>([])
    const [uploading, setUploading] = useState(false)
    const [draggingOver, setDraggingOver] = useState(false)
    const textareaRef = useRef<HTMLTextAreaElement>(null)
    const fileInputRef = useRef<HTMLInputElement>(null)
    const dragDepthRef = useRef(0)

    // Starting an edit loads the message into the composer; ending one (sent or cancelled) empties
    // it, so the edited text isn't left behind as a draft
    const editKey = editing?.key
    const wasEditingRef = useRef(false)
    useEffect(() => {
        if (editing) {
            setValue(editing.text)
            setImages(editing.images)
            setFileIds(editing.fileIds)
            textareaRef.current?.focus()
        } else if (wasEditingRef.current) {
            setValue('')
            setImages([])
            setFileIds([])
        }
        wasEditingRef.current = editing != null
        // eslint-disable-next-line react-hooks/exhaustive-deps -- reacts to a new edit, not to the object's identity
    }, [editKey])

    useEffect(() => {
        const el = textareaRef.current
        if (!el) return

        el.style.height = 'auto'
        el.style.height = `${el.scrollHeight}px`
    }, [value])

    // Primitives, so the effect runs when the picked model changes and not whenever the parent builds a new object
    const startProvider = startModel?.provider
    const startName = startModel?.model
    useEffect(() => {
        let cancelled = false
        getThinkingCapability(chatId, startName && startProvider ? {model: startName, provider: startProvider} : null).then(
            (capability) => {
                if (cancelled) return
                setCanThink(capability.kind !== 'unsupported')
                if (capability.kind === 'graduated') {
                    setThinkingModes(capability.modes)
                    // The chosen level stays only while the model offers it
                    setThinkMode((current) => (current != null && capability.modes.includes(current) ? current : capability.modes[0] ?? null))
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
    }, [chatId, modelChoice?.provider, modelChoice?.model, modelChoice?.profileId, startProvider, startName])

    // The model button's levels: a graduated model's own, or on and off; off is always last
    const thinking: ThinkingLevels | null = !canThink ? null : thinkingModes
        ? {levels: [...thinkingModes, 'off'], selected: think ? thinkMode ?? thinkingModes[0] ?? 'off' : 'off'}
        : {levels: ['on', 'off'], selected: think ? 'on' : 'off'}
    const chooseThinking = (level: string) => {
        setThink(level !== 'off')
        if (thinkingModes?.includes(level)) setThinkMode(level)
    }

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
            {editing ? (
                <Div className="composer__editing">
                    <Label variant="secondary" text="Editing a message — sending replaces it and everything after it"/>
                    <Button variant="secondary" text="Cancel" onClicked={() => onCancelEdit?.()}/>
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
                <Button className="composer__icon-button" onClicked={() => fileInputRef.current?.click()}
                        disabled={dropDisabled}>
                    <AttachmentIcon width={20} height={20}/>
                </Button>
                <Div className="composer__hint">
                    <Label variant="secondary" text="[Enter] Send"/>
                    <Label variant="secondary" text="[Shift+Enter] Newline"/>
                </Div>
                <Div className="composer__controls">
                    {onModelPicked ? (
                        <ModelMenu current={modelChoice ?? null} onPick={onModelPicked} onUseDefault={onUseDefault} defaultChoice={defaultChoice}
                                   thinking={thinking} onThinking={chooseThinking} disabled={blocked}/>
                    ) : null}
                    <Button className="composer__send" text="Send" onClicked={send} disabled={blocked || !canSend}/>
                </Div>
            </Div>
        </Div>
    )
};
