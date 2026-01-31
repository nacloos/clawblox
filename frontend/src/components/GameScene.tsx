import { Canvas } from '@react-three/fiber'
import { OrbitControls } from '@react-three/drei'
import { SpectatorObservation } from '../api'
import Arena from './Arena'
import Player from './Player'
import Entity from './Entity'

interface GameSceneProps {
  gameState: SpectatorObservation
}

export default function GameScene({ gameState }: GameSceneProps) {
  return (
    <Canvas
      camera={{ position: [80, 80, 80], fov: 50 }}
      style={{ background: '#1a1a2e' }}
    >
      <ambientLight intensity={0.5} />
      <directionalLight position={[50, 100, 50]} intensity={1} castShadow />

      <Arena />

      {gameState.players.map((player, idx) => (
        <Player key={player.id} player={player} index={idx} />
      ))}

      {gameState.entities.map((entity) => (
        <Entity key={entity.id} entity={entity} />
      ))}

      <OrbitControls
        target={[0, 0, 0]}
        maxPolarAngle={Math.PI / 2.2}
        minDistance={20}
        maxDistance={200}
      />

      <gridHelper args={[100, 50, '#444', '#333']} position={[0, 0.01, 0]} />
    </Canvas>
  )
}
