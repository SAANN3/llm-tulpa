import axios from 'axios'
import {useEffect, useState} from 'react'
import {getFileDownloadUrl} from '../../api/files/download'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'

/** Renders via the browser's PDF viewer inside an iframe pointed at a blob URL */
const PdfPreview = ({file}: PreviewerProps) => {
    const [blobUrl, setBlobUrl] = useState<string | null>(null)
    const [failed, setFailed] = useState(false)

    useEffect(() => {
        let cancelled = false
        let url: string | null = null

        axios
            .get<Blob>(getFileDownloadUrl(file.id), {responseType: 'blob'})
            .then((res) => {
                if (cancelled) return
                url = URL.createObjectURL(res.data)
                setBlobUrl(url)
            })
            .catch(() => {
                if (!cancelled) setFailed(true)
            })

        return () => {
            cancelled = true
            if (url) URL.revokeObjectURL(url)
        }
    }, [file.id])

    if (failed) {
        return (
            <Div style={{padding: 20}}>
                <Label text="Couldn't load this file."/>
            </Div>
        )
    }

    if (!blobUrl) {
        return (
            <Div style={{padding: 20}}>
                <Label variant="secondary" text="Loading…"/>
            </Div>
        )
    }

    return <iframe src={blobUrl} title={file.file_name}
                   style={{width: '100%', height: '100%', border: 'none', display: 'block'}}/>
};

export default PdfPreview
