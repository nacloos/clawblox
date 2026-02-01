import { Canvas } from '@react-three/fiber'
import { OrbitControls, Environment } from '@react-three/drei'
import { SpectatorObservation } from '../api'
import Player from './Player'
import Entity from './Entity'

interface GameSceneProps {
  gameState: SpectatorObservation
}

export default function GameScene({ gameState }: GameSceneProps) {
  return (
    <Canvas
      camera={{ position: [80, 80, 80], fov: 50 }}
      shadows
      style={{ background: 'linear-gradient(to bottom, #1a1a2e 0%, #0f0f1a 100%)' }}
    >
      <ambientLight intensity={0.4} />
      <directionalLight
        position={[50, 100, 50]}
        intensity={1.2}
        castShadow
        shadow-mapSize={[2048, 2048]}
      />
      <pointLight position={[-50, 50, -50]} intensity={0.3} color="#4a9eff" />

      {gameState.players.map((player, idx) => (
        <Player key={player.id} player={player} index={idx} />
      ))}

      {gameState.entities.map((entity) => (
        <Entity key={entity.id} entity={entity} />
      ))}

      <OrbitControls
        target={[0, 5, 0]}
        maxPolarAngle={Math.PI / 2.1}
        minDistance={20}
        maxDistance={250}
      />

      <gridHelper args={[200, 100, '#333', '#222']} position={[0, 0.01, 0]} />
      <fog attach="fog" args={['#0f0f1a', 100, 300]} />
    </Canvas>
  )
}
