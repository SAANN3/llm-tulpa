import {useCallback, useEffect, useState} from 'react'
import {getHttpsPort} from '../../api/tls/https-address.ts'
import {getTlsStatus} from '../../api/tls/status.ts'
import type {TlsStatus} from '../../api/tls/types.ts'
import {CertificateField} from '../../components/certificate-field.tsx'
import {SecurePageRow} from '../../components/secure-page-row.tsx'
import {Label, Select} from '../../components/primitives'
import {SettingsRow, SettingsSection} from '../../components/settings-fields.tsx'
import {VoiceModels} from '../../components/voice-models.tsx'
import {useAuth} from '../../context/use-auth.ts'
import {useSettings} from '../../context/use-settings.ts'
import {useSttStatus} from '../../hooks/use-stt-status.ts'

/** The language the speech is in: a code the models take, or nothing for them to find out */
const LANGUAGES: { code: string; label: string }[] = [
    {code: '', label: 'Detect it'},
    {code: 'en', label: 'English'},
    {code: 'ru', label: 'Russian'},
    {code: 'uk', label: 'Ukrainian'},
    {code: 'de', label: 'German'},
    {code: 'fr', label: 'French'},
    {code: 'es', label: 'Spanish'},
    {code: 'it', label: 'Italian'},
    {code: 'pt', label: 'Portuguese'},
    {code: 'pl', label: 'Polish'},
    {code: 'nl', label: 'Dutch'},
    {code: 'cs', label: 'Czech'},
    {code: 'tr', label: 'Turkish'},
    {code: 'ar', label: 'Arabic'},
    {code: 'hi', label: 'Hindi'},
    {code: 'zh', label: 'Chinese'},
    {code: 'ja', label: 'Japanese'},
    {code: 'ko', label: 'Korean'},
]

/** Voice input: the speech models (the owner installs them), which one and what language the user speaks, and the secure
 * connection the browser needs for the microphone. Each applies at once, apart from the Save of the other tabs. */
export const VoiceTab = () => {
    const {user} = useAuth()
    const owner = user?.role === 'owner'
    const {settings, setSettings} = useSettings()
    const {status, refresh} = useSttStatus()
    const [tls, setTls] = useState<TlsStatus | null>(null)
    const [httpsPort, setHttpsPort] = useState<number | null>(null)

    const readTls = useCallback(() => {
        if (owner) void getTlsStatus().then(setTls, () => setTls(null))
    }, [owner])
    useEffect(readTls, [readTls])
    useEffect(() => {
        void getHttpsPort().then(setHttpsPort)
    }, [])

    const installed = status?.models.filter((model) => model.installed) ?? []
    const chosen = installed.find((model) => model.id === settings?.stt_model) ?? installed.find((model) => model.id === status?.model)
    const secure = window.isSecureContext && navigator.mediaDevices?.getUserMedia != null

    return (
        <>
            <SettingsSection title="Voice models">
                {status ? <VoiceModels status={status} canManage={owner} onChanged={() => void refresh()}/> : <Label variant="secondary" text="Reading the models…"/>}
            </SettingsSection>
            <SettingsSection title="Your voice input">
                <SettingsRow label="Model" help={installed.length ? 'The model that turns your speech into text. The first installed one is used until you pick another.' : 'Install a model above to use voice input.'}>
                    {installed.length > 0
                        ? <Select values={installed.map((model) => model.label)} selected={chosen?.label}
                                  onChosen={(label) => {
                                      const model = installed.find((m) => m.label === label)
                                      if (model) void setSettings({stt_model: model.id})
                                  }}/>
                        : <Label className="voice-model__state" text="None installed"/>}
                </SettingsRow>
                <SettingsRow label="Language" help="Only some models need to be told it (Whisper); the others find it out, and this is ignored.">
                    <Select values={LANGUAGES.map((language) => language.label)}
                            selected={LANGUAGES.find((language) => language.code === (settings?.stt_language ?? ''))?.label ?? LANGUAGES[0].label}
                            onChosen={(label) => void setSettings({stt_language: LANGUAGES.find((language) => language.label === label)?.code ?? ''})}/>
                </SettingsRow>
            </SettingsSection>
            <SettingsSection title="Secure connection">
                <SecurePageRow secure={secure} port={httpsPort} addresses={tls?.addresses ?? []}/>
                {owner ? <CertificateField status={tls} onChanged={readTls}/> : null}
            </SettingsSection>
        </>
    )
};
