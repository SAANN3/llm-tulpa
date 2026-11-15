export type BlueprintSort = 'created' | 'updated'

export type Schedule =
    | {type: 'manual'}
    | {type: 'interval'; seconds: number}
    | {type: 'daily'; time: string; timezone: string}

export interface Blueprint {
    id: number
    user_id: number
    name: string
    revision: number
    schedule: Schedule
    next_due_at: string | null
    created_at: string
    updated_at: string
}

export interface BlueprintList {
    items: Blueprint[]
    total: number
    offset: number
    limit: number
}
