const ARENA_SIZE = 50
const WALL_HEIGHT = 5
const WALL_THICKNESS = 1

export default function Arena() {
  return (
    <group>
      {/* Floor */}
      <mesh rotation={[-Math.PI / 2, 0, 0]} receiveShadow>
        <planeGeometry args={[ARENA_SIZE * 2, ARENA_SIZE * 2]} />
        <meshStandardMaterial color="#2a2a3a" />
      </mesh>

      {/* Walls */}
      {/* North wall (positive Z) */}
      <mesh position={[0, WALL_HEIGHT / 2, ARENA_SIZE]}>
        <boxGeometry args={[ARENA_SIZE * 2, WALL_HEIGHT, WALL_THICKNESS]} />
        <meshStandardMaterial color="#4a4a5a" />
      </mesh>

      {/* South wall (negative Z) */}
      <mesh position={[0, WALL_HEIGHT / 2, -ARENA_SIZE]}>
        <boxGeometry args={[ARENA_SIZE * 2, WALL_HEIGHT, WALL_THICKNESS]} />
        <meshStandardMaterial color="#4a4a5a" />
      </mesh>

      {/* East wall (positive X) */}
      <mesh position={[ARENA_SIZE, WALL_HEIGHT / 2, 0]}>
        <boxGeometry args={[WALL_THICKNESS, WALL_HEIGHT, ARENA_SIZE * 2]} />
        <meshStandardMaterial color="#4a4a5a" />
      </mesh>

      {/* West wall (negative X) */}
      <mesh position={[-ARENA_SIZE, WALL_HEIGHT / 2, 0]}>
        <boxGeometry args={[WALL_THICKNESS, WALL_HEIGHT, ARENA_SIZE * 2]} />
        <meshStandardMaterial color="#4a4a5a" />
      </mesh>
    </group>
  )
}
