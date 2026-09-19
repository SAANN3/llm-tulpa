import {useEffect, useState} from 'react'
import {fetchFileBlob} from '../api/files/download'

/**
 * An object URL for a file's bytes, for handing to an `<img>`/`<video>`/`<audio>` `src`. The
 * download route needs the Bearer token, which a media element can't send, so the bytes are
 * fetched through axios and exposed as a `blob:` URL — revoked when the file changes or the
 * component unmounts. `url` is `null` while loading or after a failure.
 */
export const useFileBlobUrl = (fileId: number | null): { url: string | null; failed: boolean } => {
    const [url, setUrl] = useState<string | null>(null)
    const [failed, setFailed] = useState(false)

    useEffect(() => {
        setUrl(null)
        setFailed(false)
        if (fileId === null) return

        let cancelled = false
        let objectUrl: string | null = null

        fetchFileBlob(fileId)
            .then((blob) => {
                if (cancelled) return
                objectUrl = URL.createObjectURL(blob)
                setUrl(objectUrl)
            })
            .catch(() => {
                if (!cancelled) setFailed(true)
            })

        return () => {
            cancelled = true
            if (objectUrl) URL.revokeObjectURL(objectUrl)
        }
    }, [fileId])

    return {url, failed}
};
