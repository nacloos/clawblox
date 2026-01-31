import { SpectatorPlayerInfo } from '../api'

const PLAYER_COLORS = ['#4ecdc4', '#ff6b6b', '#ffe66d', '#95e1d3']

interface PlayerProps {
  player: SpectatorPlayerInfo
  index: number
}

export default function Player({ player, index }: PlayerProps) {
  const [x, y, z] = player.position
  const color = PLAYER_COLORS[index % PLAYER_COLORS.length]
  const healthRatio = player.health / 100

  return (
    <group position={[x, y, z]}>
      {/* Player body */}
      <mesh castShadow>
        <capsuleGeometry args={[0.5, 1, 8, 16]} />
        <meshStandardMaterial color={color} />
      </mesh>

      {/* Health bar background */}
      <mesh position={[0, 2.2, 0]}>
        <boxGeometry args={[1.2, 0.15, 0.1]} />
        <meshBasicMaterial color="#333" />
      </mesh>

      {/* Health bar fill */}
      <mesh position={[(healthRatio - 1) * 0.55, 2.2, 0.05]}>
        <boxGeometry args={[1.1 * healthRatio, 0.1, 0.1]} />
        <meshBasicMaterial color={healthRatio > 0.5 ? '#4ade80' : healthRatio > 0.25 ? '#fbbf24' : '#ef4444'} />
      </mesh>
    </group>
  )
}
