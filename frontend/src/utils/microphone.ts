/** What the microphone is asked for. With the browser's noise suppression and auto-gain on, the first words of an input were
 * misread in Firefox (they seem to settle over the first second of a stream), so they are left off and the recognizer gets
 * the raw signal. */
export const MICROPHONE_AUDIO: MediaTrackConstraints = {channelCount: 1, echoCancellation: true, noiseSuppression: false, autoGainControl: false}
