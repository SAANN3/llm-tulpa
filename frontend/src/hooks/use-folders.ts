import {useEffect, useState} from 'react'
import {createFolder as createFolderApi} from '../api/folders/create'
import {deleteFolder as deleteFolderApi} from '../api/folders/delete'
import {getFolders} from '../api/folders/get'
import {renameFolder as renameFolderApi} from '../api/folders/rename'
import type {FolderOut} from '../api/folders/types'

const FOLDERS_PAGE_SIZE = 30

/** A user's folders, ordered by their most recently active chat, optionally filtered by
 * name. Re-fetches from the top whenever `query` changes. */
export const useFolders = (query = '') => {
    const [folders, setFolders] = useState<FolderOut[]>([])
    const [total, setTotal] = useState(0)
    const [loadingMore, setLoadingMore] = useState(false)

    useEffect(() => {
        let cancelled = false
        getFolders({q: query || undefined, limit: FOLDERS_PAGE_SIZE}).then((result) => {
            if (cancelled || !('folders' in result)) return
            setFolders(result.folders)
            setTotal(result.total)
        })
        return () => {
            cancelled = true
        }
    }, [query])

    const loadOlder = async () => {
        if (loadingMore || folders.length >= total) return

        setLoadingMore(true)
        try {
            const skip = folders.length
            const result = await getFolders({q: query || undefined, skip, limit: FOLDERS_PAGE_SIZE})
            if (!('folders' in result)) return
            setFolders((prev) => [...prev, ...result.folders])
            setTotal(result.total)
        } finally {
            setLoadingMore(false)
        }
    }

    const createFolder = async (name: string) => {
        const created = await createFolderApi(name)
        setFolders((prev) => [created, ...prev])
        setTotal((t) => t + 1)
        return created
    }

    const rename = async (id: number, name: string) => {
        await renameFolderApi(id, name)
        setFolders((prev) => prev.map((f) => (f.id === id ? {...f, name} : f)))
    }

    const remove = async (id: number) => {
        await deleteFolderApi(id)
        setFolders((prev) => prev.filter((f) => f.id !== id))
        setTotal((t) => Math.max(0, t - 1))
    }

    return {folders, total, loadOlder, createFolder, rename, delete: remove}
};
