import {useState} from 'react'
import {Div, Label, Select} from '../../../components/primitives'
import {ModelPicker} from '../../../components/model-picker.tsx'
import type {StepDef, WizardContext} from '../types.ts'

/** Provider pick, then the model to chat with */
export const useModelSteps = ({persist}: WizardContext): StepDef[] => {
    const [provider, setProvider] = useState('ollama')
    const [model, setModel] = useState<string | null>(null)

    const providerStep: StepDef = {
        key: 'provider',
        title: 'LLM provider',
        canNext: true,
        body: (
            <Div className="setup__step">
                <Div className="field">
                    <Label className="field__label" text="Provider"/>
                    <Select className="setup__select" values={['ollama']} selected={provider} onChosen={setProvider}/>
                    <Label variant="secondary" className="field__help" text="Ollama runs models locally. More providers later."/>
                </Div>
            </Div>
        ),
    }

    const modelStep: StepDef = {
        key: 'model',
        title: 'Model',
        canNext: model != null,
        onNext: async () => {
            if (model) await persist({llm_provider: provider, active_model: model})
            return true
        },
        body: (active) => (
            <Div className="setup__step">
                <Label className="setup__lead" text="Pick the model to chat with — pull one if you don't have it yet."/>
                {active ? <ModelPicker selected={model} onSelect={setModel}/> : null}
            </Div>
        ),
    }

    return [providerStep, modelStep]
};
