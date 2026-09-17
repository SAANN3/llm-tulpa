import axios from 'axios'
import {useEffect, useState} from 'react'
import {read, utils, type WorkBook} from 'xlsx'
import {getFileDownloadUrl} from '../../api/files/download'
import {Button, Div, Label} from '../primitives'
import type {PreviewerProps} from './types'

/** Parses a spreadsheet via SheetJS and shows one sheet at a time as a plain table */
const XlsxPreview = ({file}: PreviewerProps) => {
    const [workbook, setWorkbook] = useState<WorkBook | null>(null)
    const [activeSheet, setActiveSheet] = useState<string | null>(null)
    const [failed, setFailed] = useState(false)

    useEffect(() => {
        let cancelled = false

        axios
            .get<ArrayBuffer>(getFileDownloadUrl(file.id), {responseType: 'arraybuffer'})
            .then((res) => {
                if (cancelled) return
                const wb = read(res.data, {type: 'array'})
                setWorkbook(wb)
                setActiveSheet(wb.SheetNames[0] ?? null)
            })
            .catch(() => {
                if (!cancelled) setFailed(true)
            })

        return () => {
            cancelled = true
        }
    }, [file.id])

    if (failed) {
        return (
            <Div style={{padding: 20}}>
                <Label text="Couldn't read this spreadsheet."/>
            </Div>
        )
    }

    if (!workbook || !activeSheet) {
        return (
            <Div style={{padding: 20}}>
                <Label variant="secondary" text="Loading…"/>
            </Div>
        )
    }

    const rows = utils.sheet_to_json<unknown[]>(workbook.Sheets[activeSheet], {header: 1, defval: ''})

    return (
        <Div>
            {workbook.SheetNames.length > 1 ? (
                <Div style={{
                    display: 'flex',
                    gap: 4,
                    padding: 8,
                    flexWrap: 'wrap',
                    borderBottom: '1px solid var(--color-border)'
                }}>
                    {workbook.SheetNames.map((name) => (
                        <Button
                            key={name}
                            text={name}
                            variant={name === activeSheet ? 'primary' : 'secondary'}
                            onClicked={() => setActiveSheet(name)}
                            style={{fontSize: 12, padding: '4px 8px'}}
                        />
                    ))}
                </Div>
            ) : null}
            <Div style={{padding: 8, background: 'white', overflow: 'auto'}}>
                <table style={{borderCollapse: 'collapse', fontSize: 12, color: 'black'}}>
                    <tbody>
                    {rows.map((row, i) => (
                        <tr key={i}>
                            {row.map((cell, j) => (
                                <td key={j}
                                    style={{border: '1px solid #ccc', padding: '2px 6px', whiteSpace: 'nowrap'}}>
                                    {String(cell)}
                                </td>
                            ))}
                        </tr>
                    ))}
                    </tbody>
                </table>
            </Div>
        </Div>
    )
};

export default XlsxPreview
