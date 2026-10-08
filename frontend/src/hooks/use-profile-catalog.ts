import {useEffect, useState} from 'react'
import {listProfiles} from '../api/profiles/list'
import type {LaunchProfile} from '../api/profiles/types'
import {listManagedModels} from '../api/runtime/models'
import type {ManagedModel} from '../api/runtime/types'

/** The models the backend runs itself and their launch profiles, loaded once per mount */
export const useProfileCatalog = () => {
    const [models, setModels] = useState<ManagedModel[]>([])
    const [profiles, setProfiles] = useState<LaunchProfile[]>([])

    useEffect(() => {
        let cancelled = false
        Promise.all([listManagedModels(), listProfiles()])
            .then(([nextModels, nextProfiles]) => {
                if (cancelled) return
                setModels(nextModels)
                setProfiles(nextProfiles)
            })
            .catch(() => {
                // Without the list the picker shows nothing and a header falls back to the model's own name.
            })
        return () => {
            cancelled = true
        }
    }, [])

    return {models, profiles}
};
