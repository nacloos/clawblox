export interface SpectatorPlayerInfo {
  id: string
  name: string
  position: [number, number, number]
  root_part_id?: number
  health: number
  attributes?: Record<string, unknown>
}

export interface SpectatorEntity {
  id: number
  entity_type: string
  position: [number, number, number]
  rotation?: [[number, number, number], [number, number, number], [number, number, number]]
  size?: [number, number, number]
  render: {
    kind: string
    role: string
    preset_id?: string
    primitive: string
    material: string
    color: [number, number, number]
    static: boolean
    casts_shadow: boolean
    receives_shadow: boolean
    visible: boolean
    double_sided: boolean
    transparency?: number
  }
  model_url?: string
  model_yaw_offset_deg?: number
  name?: string
}

export interface SpectatorObservation {
  tick: number
  game_status: string
  players: SpectatorPlayerInfo[]
  entities: SpectatorEntity[]
}
