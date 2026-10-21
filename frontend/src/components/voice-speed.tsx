import {useEffect, useRef, useState, type CSSProperties} from 'react'
import '../styles/model-picker.scss'
import '../styles/voice.scss'
import {saveSttTuning} from '../api/stt/save-tuning.ts'
import {startSttBenchmark} from '../api/stt/start-benchmark.ts'
import type {SttBenchmark, SttStatus} from '../api/stt/types.ts'
import {MIN_SAMPLE_SECONDS, useSampleRecorder} from '../hooks/use-sample-recorder.ts'
import {errorReason} from '../utils/error-reason.ts'
import {Button, Div, Label, Slider} from './primitives'
import {SettingsRow} from './settings-fields.tsx'
import {VoiceStatus} from './voice-status.tsx'

export interface VoiceSpeedProps {
    status: SttStatus
    /** Called after a measurement was started: the status is read again */
    onChanged: () => void
}

/** The idle time goes up to this many minutes on the slider (the backend takes up to a day) */
const MAX_IDLE_SLIDER = 120
const IDLE_STEP = 5
/** A change of a slider is saved when it has stood still this long, so dragging doesn't send a request for every step */
const SAVE_AFTER_MS = 600

const idleText = (minutes: number): string => (minutes === 0 ? 'never' : `${minutes} min`)

/** How far the measurement is, 0 to 1 */
const fractionOf = (bench: SttBenchmark): number => (bench.total > 0 ? bench.done / bench.total : 0)

/** Settings → Voice, owner only: how many CPU threads the models read with and how long an idle one stays loaded, and a
 * measurement of how fast each installed model reads at each thread count, on a few seconds of the owner's own voice */
export const VoiceSpeed = ({status, onChanged}: VoiceSpeedProps) => {
    const [threads, setThreads] = useState(status.tuning.threads)
    const [idle, setIdle] = useState(status.tuning.idle_unload_minutes)
    const [error, setError] = useState<string | null>(null)
    const saveTimer = useRef(0)
    const recorder = useSampleRecorder()
    const bench = status.benchmark
    const measuring = bench?.state === 'running'
    const installed = status.models.some((model) => model.installed)

    const save = (next: { threads: number; idle_unload_minutes: number }) => {
        window.clearTimeout(saveTimer.current)
        saveTimer.current = window.setTimeout(() => {
            setError(null)
            saveSttTuning(next).then(() => onChanged(), (e) => setError(errorReason(e, 'The setting could not be saved.')))
        }, SAVE_AFTER_MS)
    }
    useEffect(() => () => window.clearTimeout(saveTimer.current), [])

    const changeThreads = (value: number) => {
        setThreads(value)
        save({threads: value, idle_unload_minutes: idle})
    }
    const changeIdle = (value: number) => {
        setIdle(value)
        save({threads, idle_unload_minutes: value})
    }

    const measure = async () => {
        if (!recorder.sample) return
        setError(null)
        try {
            await startSttBenchmark(recorder.sample)
            // The recording has done its part: it is not kept
            recorder.discard()
        } catch (e) {
            setError(errorReason(e, 'The measurement could not be started.'))
        }
        onChanged()
    }

    const tooShort = recorder.sample != null && recorder.seconds < MIN_SAMPLE_SECONDS
    const below = recorder.state === 'recording' ? (
        <>
            <Label className="voice-model__state" text="Read a sentence or two aloud, in the voice and language you dictate in."/>
            <VoiceStatus state="listening" levels={recorder.levels} startedAt={recorder.startedAt} onStop={recorder.stop} escCancels={false}/>
        </>
    ) : measuring && bench ? (
        <>
            <Label className="voice-model__state" text={`Reading with ${bench.note ?? '…'} · ${bench.done} of ${bench.total}`}/>
            <Div className="model-picker__bar">
                <Div className="model-picker__bar-fill" style={{'--fraction': fractionOf(bench)} as CSSProperties}/>
            </Div>
        </>
    ) : (
        <>
            {recorder.sample ? (
                <Div className="voice-speed__sample">
                    <Label text={`Recorded ${recorder.seconds.toFixed(1)} s`}/>
                    <Button text="Measure" disabled={tooShort} onClicked={() => void measure()}/>
                    <Button variant="secondary" text="Discard" onClicked={recorder.discard}/>
                </Div>
            ) : null}
            {tooShort ? <Label className="voice-model__error" text={`That is too short: record at least ${MIN_SAMPLE_SECONDS} seconds.`}/> : null}
            {recorder.error ? <Label className="voice-model__error" text={recorder.error}/> : null}
            {bench?.state === 'failed' && bench.error ? <Label className="voice-model__error" text={`The measurement failed: ${bench.error}`}/> : null}
            {bench && bench.models.some((model) => model.runs.length > 0) ? (
                <BenchTable bench={bench} current={threads} onUse={changeThreads}/>
            ) : null}
        </>
    )

    return (
        <>
            <SettingsRow label="CPU threads" stack help={`How many of this machine's ${status.cores} cores a model reads with. More is faster up to a point; the rest stay free for the language model. Applies at once.`}>
                <Div className="voice-speed__slider">
                    <Slider value={threads} onChanged={changeThreads} min={1} max={Math.max(1, status.cores)}/>
                    <Label className="voice-speed__value" text={String(threads)}/>
                </Div>
            </SettingsRow>
            <SettingsRow label="Unload when idle" stack help="How long a model that nobody is using stays in memory (a gigabyte or two) before it is let go of. It loads again, in a couple of seconds, on the next use.">
                <Div className="voice-speed__slider">
                    <Slider value={idle} onChanged={changeIdle} min={0} max={MAX_IDLE_SLIDER} step={IDLE_STEP}/>
                    <Label className="voice-speed__value" text={idleText(idle)}/>
                </Div>
            </SettingsRow>
            <SettingsRow label="Measure speed" top below={below}
                         help={installed
                             ? 'Record a few seconds of your voice. Every installed model reads it at several thread counts, and the table shows which count is fast enough. It takes all the cores for a minute or more, so do it when nothing else is running.'
                             : 'Install a model above to measure it.'}>
                {recorder.state === 'idle' && !measuring
                    ? <Button variant="secondary" text={recorder.sample ? 'Record again' : 'Record a sample'} disabled={!installed} onClicked={recorder.start}/>
                    : <Label className="voice-model__state" text={recorder.state === 'starting' ? 'Starting…' : measuring ? 'Measuring…' : 'Recording…'}/>}
            </SettingsRow>
            {error ? <Label className="voice-model__error" text={error}/> : null}
        </>
    )
}

/** What the last measurement found: for each model, how many times faster than real time it read at each thread count */
const BenchTable = ({bench, current, onUse}: { bench: SttBenchmark; current: number; onUse: (threads: number) => void }) => (
    <Div className="voice-speed__table">
        <Label className="voice-model__state" text={`Measured on ${bench.audio_seconds.toFixed(1)} s of speech. A speed of 1× reads speech as fast as it is said.`}/>
        {bench.models.filter((model) => model.runs.length > 0).map((model) => (
            <Div key={model.model} className="voice-speed__model">
                <Label className="voice-speed__name" text={model.label}/>
                <Div className="voice-speed__runs">
                    {model.runs.map((run) => (
                        <Label key={run.threads} className={`voice-speed__run${run.threads === model.best_threads ? ' voice-speed__run--best' : ''}`}
                               text={`${run.threads} threads: ${run.speed.toFixed(1)}×`}/>
                    ))}
                </Div>
                {model.best_threads != null && model.best_threads !== current
                    ? <Button variant="secondary" text={`Use ${model.best_threads} threads`} onClicked={() => onUse(model.best_threads as number)}/>
                    : null}
            </Div>
        ))}
    </Div>
);
