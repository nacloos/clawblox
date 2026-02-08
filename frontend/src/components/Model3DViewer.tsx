import { Suspense, useEffect } from 'react'
import { Canvas } from '@react-three/fiber'
import { OrbitControls, useGLTF, Environment } from '@react-three/drei'

interface Model3DViewerProps {
  modelPath: string
}

function Model({ url }: { url: string }) {
  const { scene } = useGLTF(url)

  // Clone the scene to avoid issues when switching models
  const clonedScene = scene.clone()

  // Center and scale the model appropriately
  return <primitive object={clonedScene} scale={1.5} position={[0, -0.5, 0]} />
}

// Preload models to avoid loading delays
useGLTF.preload('/models/player.glb')
useGLTF.preload('/models/zucc.glb')

function LoadingFallback() {
  return (
    <mesh>
      <boxGeometry args={[1, 1, 1]} />
      <meshStandardMaterial color="#888" wireframe />
    </mesh>
  )
}

export default function Model3DViewer({ modelPath }: Model3DViewerProps) {
  return (
    <div className="w-full h-full bg-muted/20 rounded-xl overflow-hidden relative">
      <Canvas camera={{ position: [0, 0, 5], fov: 50 }}>
        <Suspense fallback={<LoadingFallback />}>
          <ambientLight intensity={0.5} />
          <spotLight position={[10, 10, 10]} angle={0.15} penumbra={1} />
          <pointLight position={[-10, -10, -10]} />
          {/* Key prop forces remount when model changes */}
          <Model key={modelPath} url={modelPath} />
          <OrbitControls
            enablePan={false}
            enableZoom={true}
            minDistance={3}
            maxDistance={8}
          />
          <Environment preset="sunset" />
        </Suspense>
      </Canvas>
      <div className="absolute bottom-4 right-4 text-xs text-muted-foreground bg-background/80 px-2 py-1 rounded">
        {modelPath.split('/').pop()}
      </div>
    </div>
  )
}
