import {useState} from 'react'
import type {Installed} from '../../../api/runtime/install'
import {EngineSetup} from '../../../components/engine-setup.tsx'
import {LlamaModelPick} from '../../../components/llama-model-pick.tsx'
import {Div, Label, Select} from '../../../components/primitives'
import {ModelPicker} from '../../../components/model-picker.tsx'
import type {StepDef, WizardContext} from '../types.ts'

const LLAMA = 'llama.cpp (built in)'
const OLLAMA = 'ollama'

/** Where the model runs, then (for the built-in llama.cpp) its engine and the model file, or (for
 * Ollama) the model to pull or choose */
export const useModelSteps = ({persist}: WizardContext): { steps: StepDef[]; llamaProfileId: number | null } => {
    const [provider, setProvider] = useState(LLAMA)
    const [installed, setInstalled] = useState<Installed | null>(null)
    const [llamaModel, setLlamaModel] = useState<string | null>(null)
    const [llamaProfileId, setLlamaProfileId] = useState<number | null>(null)
    const [ollamaModel, setOllamaModel] = useState<string | null>(null)

    const providerStep: StepDef = {
        key: 'provider',
        title: 'LLM provider',
        canNext: true,
        body: (
            <Div className="setup__step">
                <Div className="field">
                    <Label className="field__label" text="Provider"/>
                    <Select className="setup__select" values={[LLAMA, OLLAMA]} selected={provider} onChosen={setProvider}/>
                    <Label variant="secondary" className="field__help"
                           text="The built-in llama.cpp is downloaded and run by the backend, and every model setting is editable in the app. Ollama is for those who already run it."/>
                </Div>
            </Div>
        ),
    }

    if (provider === OLLAMA) {
        return {llamaProfileId: null, steps: [providerStep, {
            key: 'model',
            title: 'Model',
            canNext: ollamaModel != null,
            onNext: async () => {
                if (ollamaModel) await persist({llm_provider: 'ollama', active_model: ollamaModel})
                return true
            },
            body: (active) => (
                <Div className="setup__step">
                    <Label className="setup__lead" text="Pick the model to chat with — pull one if you don't have it yet."/>
                    {active ? <ModelPicker selected={ollamaModel} onSelect={setOllamaModel}/> : null}
                </Div>
            ),
        }]}
    }

    return {llamaProfileId, steps: [
        providerStep,
        {
            key: 'engine',
            title: 'llama.cpp',
            canNext: installed != null,
            body: (active) => (active ? <EngineSetup onInstalled={setInstalled}/> : null),
        },
        {
            key: 'llama-model',
            title: 'Model',
            canNext: llamaModel != null,
            onNext: async () => {
                if (llamaModel) await persist({llm_provider: 'llama-cpp', active_model: llamaModel})
                return true
            },
            body: (active) => (active ? <LlamaModelPick onPicked={(file, profileId) => {
                setLlamaModel(file)
                setLlamaProfileId(profileId)
            }}/> : null),
        },
    ]}
};
