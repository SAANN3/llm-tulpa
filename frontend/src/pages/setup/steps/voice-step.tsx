import type {StepDef} from '../types.ts'
import {VoiceStepBody} from './voice-step-body.tsx'

/** Offers the speech models for voice input; skipping it leaves the microphone button switched off */
export const useVoiceStep = (): StepDef => ({
    key: 'voice',
    since: 2,
    group: 'Voice',
    title: 'Voice input',
    canNext: true,
    primaryLabel: 'Next',
    body: <VoiceStepBody/>,
});
