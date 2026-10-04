import {useState} from 'react'
import type {Installed} from '../../../api/runtime/install'
import {EngineSetup} from '../../../components/engine-setup.tsx'
import {registerModel} from '../../../api/runtime/register'
import {LlamaModelPick, type LlamaPick} from '../../../components/llama-model-pick.tsx'
import {HfPanel} from '../../../components/models/hf-panel.tsx'
import {OllamaAddressField} from '../../../components/ollama-address-field.tsx'
import {Div, Label, RadioButton} from '../../../components/primitives'
import {ModelPicker} from '../../../components/model-picker.tsx'
import {errorReason} from '../../../utils/error-reason.ts'
import type {StepDef, WizardContext} from '../types.ts'

const LLAMA = 'llama.cpp (built in)'
const OLLAMA = 'ollama'

// Radio buttons rather than a dropdown: the wizard's slides clip what sticks out of them, and a dropdown
// opened on this short step was cut off
const PROVIDERS = [
    {id: LLAMA, help: 'Downloaded and run by the backend, and every model setting is editable in the app.'},
    {id: OLLAMA, help: 'For those who already run Ollama.'},
]

/** Where the model runs, then (for the built-in llama.cpp) its engine and the model file, or (for
 * Ollama) the model to pull or choose */
export const useModelSteps = ({persist}: WizardContext): { steps: StepDef[]; llamaProfileId: number | null } => {
    const [provider, setProvider] = useState(LLAMA)
    const [installed, setInstalled] = useState<Installed | null>(null)
    const [llamaPicks, setLlamaPicks] = useState<LlamaPick[]>([])
    const [llamaError, setLlamaError] = useState<string | null>(null)
    const [llamaProfileId, setLlamaProfileId] = useState<number | null>(null)
    // The model step's second view, for downloading a model; Back and Done return to the list
    const [downloadView, setDownloadView] = useState(false)
    const [downloading, setDownloading] = useState(false)
    const [ollamaModel, setOllamaModel] = useState<string | null>(null)
    const [ollamaError, setOllamaError] = useState<string | null>(null)
    // Bumped when Ollama's address changes, so its list of models is read again from the new one
    const [ollamaKey, setOllamaKey] = useState(0)

    const providerStep: StepDef = {
        key: 'provider',
        title: 'LLM provider',
        canNext: true,
        body: (
            <Div className="setup__step">
                <Label className="field__label" text="Provider"/>
                {PROVIDERS.map((option) => (
                    <Div key={option.id} className={`setup__choice${provider === option.id ? ' setup__choice--active' : ''}`}
                         onClick={() => setProvider(option.id)}>
                        <RadioButton name="provider" value={option.id} checked={provider === option.id} onChanged={setProvider}/>
                        <Div className="setup__choice-text">
                            <Label text={option.id}/>
                            <Label variant="secondary" className="field__help" text={option.help}/>
                        </Div>
                    </Div>
                ))}
            </Div>
        ),
    }

    if (provider === OLLAMA) {
        return {llamaProfileId: null, steps: [providerStep, {
            key: 'model',
            title: 'Model',
            canNext: ollamaModel != null,
            onNext: async () => {
                setOllamaError(null)
                try {
                    if (ollamaModel) await persist({llm_provider: 'ollama', active_model: ollamaModel})
                } catch (e) {
                    // The backend refuses a model Ollama doesn't have; staying on the step with the reason
                    // beats a Next button that silently does nothing
                    setOllamaError(errorReason(e, 'Could not save the chosen model.'))
                    return false
                }
                return true
            },
            body: (active) => (
                <Div className="setup__step">
                    <Label className="setup__lead" text="Pick the model to chat with — pull one if you don't have it yet."/>
                    {active ? <OllamaAddressField onSaved={() => setOllamaKey((k) => k + 1)}/> : null}
                    {active ? <ModelPicker key={ollamaKey} selected={ollamaModel} onSelect={setOllamaModel} onDeselect={() => setOllamaModel(null)}/> : null}
                    {ollamaError ? <Label className="model-picker__error" text={ollamaError}/> : null}
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
            title: downloadView ? 'Download a model' : 'Model',
            canNext: downloadView ? !downloading : llamaPicks.length > 0,
            primaryLabel: downloadView ? 'Done' : undefined,
            // Leaving while a file is arriving would orphan the download's progress, so both buttons wait for it
            locked: downloadView && downloading,
            onBack: () => {
                if (!downloadView) return false
                setDownloadView(false)
                return true
            },
            onNext: async () => {
                if (downloadView) {
                    setDownloadView(false)
                    return false
                }
                // Everything chosen is written now; registering a file that is already registered is harmless,
                // so going back and forward again doesn't duplicate anything
                setLlamaError(null)
                try {
                    let firstProfile: number | null = null
                    for (const pick of llamaPicks) {
                        const model = await registerModel(pick.file, undefined, pick.projector ?? undefined)
                        firstProfile ??= model.profile_ids[0] ?? null
                    }
                    setLlamaProfileId(firstProfile)
                } catch (e) {
                    setLlamaError(errorReason(e, 'Could not add the chosen models.'))
                    return false
                }
                try {
                    await persist({llm_provider: 'llama-cpp', active_model: llamaPicks[0].file})
                } catch (e) {
                    setLlamaError(errorReason(e, 'Could not save the default model.'))
                    return false
                }
                return true
            },
            body: (active) => {
                if (!active) return null
                // The list is mounted afresh on the way back, so a file that just arrived is in it
                return downloadView
                    ? <HfPanel onDownloaded={() => undefined} onRunningChange={setDownloading}
                               doneHint="downloaded — it will be in the list when you go back"/>
                    : <LlamaModelPick picks={llamaPicks} onChange={setLlamaPicks} onDownload={() => setDownloadView(true)} error={llamaError}/>
            },
        },
    ]}
};
