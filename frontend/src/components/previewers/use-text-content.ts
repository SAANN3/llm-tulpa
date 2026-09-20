import axios from 'axios'
import {useEffect, useState} from 'react'
import {getFileDownloadUrl} from '../../api/files/download'
import type {FileOut} from '../../api/files/types'

/** Fetches a file's content as plain text, shared by every text-based previewer */
export const useTextContent = (file: FileOut): { content: string | null; failed: boolean } => {
    const [content, setContent] = useState<string | null>(null)
    const [failed, setFailed] = useState(false)

    useEffect(() => {
        let cancelled = false
        setContent(null)
        setFailed(false)

        axios
            .get<string>(getFileDownloadUrl(file.id), {responseType: 'text'})
            .then((res) => {
                if (!cancelled) setContent(res.data)
            })
            .catch(() => {
                if (!cancelled) setFailed(true)
            })

        return () => {
            cancelled = true
        }
    }, [file.id])

    return {content, failed}
};
