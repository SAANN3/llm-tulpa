import {useEffect, useState} from 'react'
import {Close, File as FileIcon} from 'pixelarticons/react'
import {saveFile} from '../api/files/download'
import {getFile} from '../api/files/get'
import type {FileOut} from '../api/files/types'
import {useFileBlobUrl} from '../hooks/use-file-blob-url.ts'
import {getFileExtension} from '../utils/file-extension.ts'
import {AttachmentPreview} from './attachment-preview.tsx'
import {Button, Div, Label} from './primitives'
import {getMediaKind} from './previewers/registry'
import {WindowsPopup} from './windows-popup.tsx'

export type AttachmentProps = {
    size?: number
    onRemove?: () => void
} & (
    | { kind: 'image'; image: string }
    | {
    kind: 'file'
    fileId: number
}
    )

const DEFAULT_SIZE = 56

const DEFAULT_FILE_PREVIEW_SIZE = {width: 720, height: 520}

/** Builds a data URL for an image, detecting SVG since browsers don't sniff it */
const toDataUrl = (image: string): string => {
    const mimeType = looksLikeSvg(image) ? 'image/svg+xml' : 'image/png'
    return `data:${mimeType};base64,${image}`
};

const looksLikeSvg = (base64: string): boolean => {
    try {
        return /<\s*(\?xml|svg)/i.test(atob(base64.slice(0, 200)))
    } catch {
        return false
    }
};

/** Triggers a browser download via a hidden anchor click */
const triggerDownload = (href: string, filename: string): void => {
    const a = document.createElement('a')
    a.href = href
    a.download = filename
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
};

const FILE_ICON = <FileIcon width={20} height={20}/>

/** One attachment: a removable thumbnail while composing, or a plain one on a sent message */
export const Attachment = (props: AttachmentProps) => {
    const {size = DEFAULT_SIZE, onRemove} = props
    const [previewOpen, setPreviewOpen] = useState(false)
    const [file, setFile] = useState<FileOut | null>(null)

    useEffect(() => {
        if (props.kind !== 'file') return
        let cancelled = false
        setFile(null)

        getFile(props.fileId).then((result) => {
            if (!cancelled) setFile(result)
        })

        return () => {
            cancelled = true
        }
    }, [props.kind === 'file' ? props.fileId : null])

    const download = () => {
        if (props.kind === 'image') triggerDownload(toDataUrl(props.image), 'image.png')
        else if (file) void saveFile(file.id, file.file_name)
    }

    const mediaKind = props.kind === 'file' && file ? getMediaKind(getFileExtension(file.file_name)) : null
    // The thumbnail's bytes come through axios (the Bearer token can't ride on a bare `src`).
    // Images only: a video would have to be downloaded whole just to draw a chip, so it gets the
    // generic file chip and loads when its preview is opened.
    const {url: mediaUrl} = useFileBlobUrl(mediaKind === 'image' && file ? file.id : null)
    const thumbnailStyle = {
        width: '100%',
        height: '100%',
        objectFit: 'cover' as const,
        borderRadius: 0,
        border: '1px solid var(--color-border)',
        cursor: 'zoom-in',
    }

    return (
        <>
            <Div style={{position: 'relative', width: size, height: size}}>
                {props.kind === 'image' ? (
                    <img src={toDataUrl(props.image)} alt="" onClick={() => setPreviewOpen(true)}
                         style={thumbnailStyle}/>
                ) : mediaKind === 'image' && file && mediaUrl ? (
                    <img src={mediaUrl} alt={file.file_name} onClick={() => setPreviewOpen(true)}
                         style={thumbnailStyle}/>
                ) : (
                    <Div
                        className="vbox center"
                        onClick={() => setPreviewOpen(true)}
                        variant="secondary"
                        style={{
                            width: '100%',
                            height: '100%',
                            boxSizing: 'border-box',
                            gap: 4,
                            padding: 4,
                            borderRadius: 0,
                            border: '1px solid var(--color-border)',
                            cursor: 'pointer',
                        }}
                    >
                        {FILE_ICON}
                        <Label
                            text={file?.file_name ?? '...'}
                            style={{
                                fontSize: 9,
                                lineHeight: 1.2,
                                textAlign: 'center',
                                wordBreak: 'break-all',
                                display: '-webkit-box',
                                WebkitLineClamp: 2,
                                WebkitBoxOrient: 'vertical',
                                overflow: 'hidden',
                            }}
                        />
                    </Div>
                )}
                {onRemove ? (
                    <Button
                        onClicked={onRemove}
                        style={{
                            position: 'absolute',
                            top: -6,
                            right: -6,
                            width: 18,
                            height: 18,
                            padding: 0,
                            borderRadius: 0,
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                        }}
                    >
                        <Close width={12} height={12}/>
                    </Button>
                ) : null}
            </Div>
            <WindowsPopup
                open={previewOpen}
                onClose={() => setPreviewOpen(false)}
                title={props.kind === 'image' ? 'Preview' : file?.file_name ?? 'File'}
                onDownload={props.kind === 'image' || file ? download : undefined}
                defaultSize={props.kind === 'file' ? DEFAULT_FILE_PREVIEW_SIZE : undefined}
            >
                {props.kind === 'image' ? (
                    <img
                        src={toDataUrl(props.image)}
                        alt=""
                        style={{
                            display: 'block',
                            width: '100%',
                            height: '100%',
                            maxWidth: '90vw',
                            maxHeight: '90vh',
                            objectFit: 'contain'
                        }}
                    />
                ) : file ? (
                    <AttachmentPreview file={file}/>
                ) : (
                    <Div style={{padding: 20}}>
                        <Label variant="secondary" text="Loading…"/>
                    </Div>
                )}
            </WindowsPopup>
        </>
    )
};
