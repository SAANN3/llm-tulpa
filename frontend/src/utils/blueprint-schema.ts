import type {InputBinding, Literal} from '../api/blueprints/types'
import type {PropertySchema} from '../api/plugins/types'

export const emptyLiteral = (schema: PropertySchema): Literal => {
    switch (schema.property_type) {
        case 'string': return ''
        case 'integer': case 'number': return 0
        case 'boolean': return false
        case 'array': return []
        case 'object': return Object.fromEntries((schema.properties ?? []).filter(item => item.required).map(item => [item.name, emptyLiteral(item)]))
    }
}

export const literalBinding = (schema: PropertySchema): InputBinding => ({source: 'literal', value: emptyLiteral(schema)})

export const namedKey = (value: string): boolean =>
    value.length > 0 && value.length <= 128 && [...value].every(char => /[A-Za-z0-9_.-]/.test(char))
