import {useCallback, useEffect, useState} from 'react'
import {listLocalFiles} from '../api/llm/local-files'
import type {LocalFiles} from '../api/llm/types'
import {listProfiles} from '../api/profiles/list'
import type {LaunchProfile} from '../api/profiles/types'
import {listManagedModels} from '../api/runtime/models'
import type {ManagedModel} from '../api/runtime/types'

/**
 * The models the backend runs itself and their launch profiles, plus (owner only) the `.gguf` files
 * on disk that could be added. `reload` rereads all of it after a change.
 */
export const useModelsData = (isOwner: boolean) => {
    const [models, setModels] = useState<ManagedModel[]>([])
    const [profiles, setProfiles] = useState<LaunchProfile[]>([])
    const [files, setFiles] = useState<LocalFiles | null>(null)
    const [error, setError] = useState<string | null>(null)

    const reload = useCallback(async () => {
        try {
            const [nextModels, nextProfiles] = await Promise.all([listManagedModels(), listProfiles()])
            setModels(nextModels)
            setProfiles(nextProfiles)
            setError(null)
        } catch {
            setError('Could not load the models.')
        }
        if (isOwner) {
            try {
                setFiles(await listLocalFiles())
            } catch {
                setFiles(null)
            }
        }
    }, [isOwner])

    useEffect(() => {
        void reload()
    }, [reload])

    return {models, profiles, files, error, reload}
};
