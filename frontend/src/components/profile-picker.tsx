import '../styles/model-picker.scss'
import type {LaunchProfile} from '../api/profiles/types'
import {useProfileCatalog} from '../hooks/use-profile-catalog.ts'
import {profileSummary} from '../utils/profile-summary.ts'
import {Button, Div, Label} from './primitives'

export interface ProfilePickerProps {
    selectedProfileId: number | null
    onSelect: (profile: LaunchProfile) => void
}

/** The models the backend runs itself, each with its launch profiles to choose from */
export const ProfilePicker = ({selectedProfileId, onSelect}: ProfilePickerProps) => {
    const {models, profiles} = useProfileCatalog()

    if (models.length === 0) {
        return <Label variant="secondary" className="model-picker__meta"
                      text="No model has been added to the backend yet — the owner adds them on the Models page."/>
    }

    return (
        <Div className="model-picker__list">
            {models.map((model) => (
                <Div key={model.id}>
                    <Label variant="secondary" className="model-picker__section" text={model.display_name ?? model.file}/>
                    {profiles.filter((p) => model.profile_ids.includes(p.id)).map((profile) => (
                        <Div key={profile.id}
                             className={`model-picker__row${profile.id === selectedProfileId ? ' model-picker__row--active' : ''}`}>
                            <Div className="model-picker__info">
                                <Label className="model-picker__name" text={profile.name}/>
                                <Label variant="secondary" className="model-picker__meta" text={profileSummary(profile)}/>
                            </Div>
                            {profile.id === selectedProfileId ? (
                                <Label variant="secondary" className="model-picker__active-tag" text="active"/>
                            ) : (
                                <Button variant="secondary" text="Select" onClicked={() => onSelect(profile)}/>
                            )}
                        </Div>
                    ))}
                </Div>
            ))}
        </Div>
    )
};
