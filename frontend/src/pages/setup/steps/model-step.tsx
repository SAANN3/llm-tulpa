import {useState} from 'react'
import type {Installed} from '../../../api/runtime/install'
import {EngineSetup} from '../../../components/engine-setup.tsx'
import {registerModel} from '../../../api/runtime/register'
import {LlamaModelPick, type LlamaPick} from '../../../components/llama-model-pick.tsx'
import {HfPanel} from '../../../components/models/hf-panel.tsx'
import {OllamaAddressField} from '../../../components/ollama-address-field.tsx'
import {ChoiceGroup, Div, Label} from '../../../components/primitives'
import {ModelPicker} from '../../../components/model-picker.tsx'
import {errorReason} from '../../../utils/error-reason.ts'
import type {StepDef, WizardContext} from '../types.ts'

const LLAMA = 'llama.cpp (built in)'
const OLLAMA = 'ollama'

// Each with what it means, read together with the choice
const PROVIDERS = [
    {value: LLAMA, label: LLAMA, help: 'Downloaded and run by the backend, and every model setting is editable in the app.'},
    {value: OLLAMA, label: OLLAMA, help: 'For those who already run Ollama.'},
]

/** Where the model runs, then (for the built-in llama.cpp) its engine and the model file, or (for
 * Ollama) the model to pull or choose */
export const useModelSteps = ({persist}: WizardContext): { steps: StepDef[]; llamaProfileId: number | null; summary: string | null } => {
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
        group: 'Model',
        part: 'Provider',
        title: 'Model · Provider',
        canNext: true,
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="Where does the model run?"/>
                <ChoiceGroup label="Where the model runs" options={PROVIDERS} chosen={provider} onChosen={setProvider}/>
            </Div>
        ),
    }

    if (provider === OLLAMA) {
        return {llamaProfileId: null, summary: ollamaModel != null ? `${ollamaModel} on Ollama` : null, steps: [providerStep, {
            key: 'model',
            group: 'Model',
            part: 'Model',
            title: 'Model · Model',
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
            body: (
                <Div className="setup__step">
                    <Label className="setup__lead" text="Pick the model to chat with — pull one if you don't have it yet."/>
                    <OllamaAddressField onSaved={() => setOllamaKey((k) => k + 1)}/>
                    <ModelPicker key={ollamaKey} selected={ollamaModel} onSelect={setOllamaModel} onDeselect={() => setOllamaModel(null)}/>
                    {ollamaError ? <Label className="model-picker__error" text={ollamaError}/> : null}
                </Div>
            ),
        }]}
    }

    const summary = llamaPicks.length > 0
        ? `${llamaPicks[0].file} on llama.cpp${installed ? ` (${installed.build})` : ''}${llamaPicks.length > 1 ? `, and ${llamaPicks.length - 1} more` : ''}`
        : null

    return {llamaProfileId, summary, steps: [
        providerStep,
        {
            key: 'engine',
            group: 'Model',
            part: 'llama.cpp',
            title: 'Model · llama.cpp',
            canNext: installed != null,
            body: <EngineSetup onInstalled={setInstalled}/>,
        },
        {
            key: 'llama-model',
            group: 'Model',
            part: 'Model file',
            title: downloadView ? 'Model · Download' : 'Model · Model file',
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
            // The list is mounted afresh on the way back, so a file that just arrived is in it
            body: downloadView
                ? <HfPanel onDownloaded={() => undefined} onRunningChange={setDownloading}
                           doneHint="downloaded — it will be in the list when you go back"/>
                : <LlamaModelPick picks={llamaPicks} onChange={setLlamaPicks} onDownload={() => setDownloadView(true)} error={llamaError}/>,
        },
    ]}
};
