import { Canvas } from '@react-three/fiber'
import { OrbitControls } from '@react-three/drei'
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
      gl={{ logarithmicDepthBuffer: true }}
      style={{ background: 'linear-gradient(to bottom, #1a1a2e 0%, #0f0f1a 100%)' }}
    >
      <hemisphereLight args={['#87ceeb', '#444444', 0.6]} />
      <ambientLight intensity={0.15} />
      <directionalLight
        position={[50, 100, 50]}
        intensity={1.2}
        castShadow
        shadow-mapSize={[2048, 2048]}
        shadow-camera-near={1}
        shadow-camera-far={250}
        shadow-camera-left={-80}
        shadow-camera-right={80}
        shadow-camera-top={80}
        shadow-camera-bottom={-80}
        shadow-bias={-0.0001}
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

      <fog attach="fog" args={['#0f0f1a', 100, 300]} />
    </Canvas>
  )
}
