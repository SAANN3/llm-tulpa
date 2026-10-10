import axios from 'axios'
import {useEffect, useState} from 'react'
import {read, utils, type WorkBook} from 'xlsx'
import {getFileDownloadUrl} from '../../api/files/download'
import {Button, Div, Label} from '../primitives'
import {useWindowTools} from '../popups/base/window-tools.ts'
import type {PreviewerProps} from './types'

/** Parses a spreadsheet via SheetJS and shows one sheet at a time as a table in the theme's colors; the sheets are in
 * the window's tool row (keys 1-9) */
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

    const sheets = workbook?.SheetNames ?? []
    const rows = workbook && activeSheet ? utils.sheet_to_json<unknown[]>(workbook.Sheets[activeSheet], {header: 1, defval: ''}) : []

    useWindowTools({
        node: sheets.length > 1 ? sheets.map((name) => (
            <Button key={name} variant="secondary" className={`preview-tool${name === activeSheet ? ' preview-tool--on' : ''}`} text={name}
                    onClicked={() => setActiveSheet(name)}/>
        )) : undefined,
        scalable: true,
        meta: workbook ? `${sheets.length > 1 ? `${sheets.length} sheets · ` : ''}${rows.length} rows` : undefined,
        actions: sheets.length > 1 ? [{
            keys: sheets.slice(0, 9).map((_, i) => String(i + 1)),
            shown: `1-${Math.min(sheets.length, 9)}`,
            label: 'sheet',
            run: (key: string) => setActiveSheet(sheets[Number(key) - 1]),
        }] : [],
    }, [workbook, activeSheet, rows.length])

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

    return (
        <table className="sheet-preview">
            <tbody>
            {rows.map((row, i) => (
                <tr key={i}>
                    <th>{i + 1}</th>
                    {row.map((cell, j) => <td key={j}>{String(cell)}</td>)}
                </tr>
            ))}
            </tbody>
        </table>
    )
};

export default XlsxPreview
