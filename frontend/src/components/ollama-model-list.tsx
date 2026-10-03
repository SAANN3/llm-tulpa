import {useEffect, useState} from 'react'
import '../styles/model-picker.scss'
import type {LocalModel} from '../api/llm/models'
import {listModels} from '../api/llm/models'
import {modelRequirements} from '../utils/model-info'
import {Button, Div, Label} from './primitives'

export interface OllamaModelListProps {
    selected?: string | null
    onSelect: (name: string) => void
}

/** The models installed in Ollama, to choose among. Pulling and importing more is on the Models page. */
export const OllamaModelList = ({selected, onSelect}: OllamaModelListProps) => {
    const [models, setModels] = useState<LocalModel[] | null>(null)
    const [error, setError] = useState<string | null>(null)

    useEffect(() => {
        listModels('ollama').then(setModels).catch(() => setError('Could not reach Ollama. Check its address on the Models page, in the Ollama tab.'))
    }, [])

    if (error) return <Label variant="secondary" className="model-picker__error" text={error}/>
    if (models?.length === 0) return <Label variant="secondary" className="model-picker__meta" text="No model is installed in Ollama."/>

    return (
        <Div className="model-picker__list">
            {(models ?? []).map((m) => (
                <Div key={m.name} className={`model-picker__row${m.name === selected ? ' model-picker__row--active' : ''}`}>
                    <Div className="model-picker__info">
                        <Label className="model-picker__name" text={m.name}/>
                        <Label variant="secondary" className="model-picker__meta" text={modelRequirements(m) || 'installed'}/>
                    </Div>
                    {m.name === selected ? (
                        <Label variant="secondary" className="model-picker__active-tag" text="active"/>
                    ) : (
                        <Button variant="secondary" text="Select" onClicked={() => onSelect(m.name)}/>
                    )}
                </Div>
            ))}
        </Div>
    )
};
