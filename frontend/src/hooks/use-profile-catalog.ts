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

/** What the header shows for a chat: the model's name, plus the profile's when the model has several */
export const profileLabel = (models: ManagedModel[], profiles: LaunchProfile[], profileId: number | null): string | null => {
    const profile = profiles.find((p) => p.id === profileId)
    if (!profile) return null
    const model = models.find((m) => m.id === profile.model_id)
    const name = model?.display_name ?? profile.model
    return (model?.profile_ids.length ?? 1) > 1 ? `${name} · ${profile.name}` : name
};
