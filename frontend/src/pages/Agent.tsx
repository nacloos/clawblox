import { useState } from 'react'
import Model3DViewer from '../components/Model3DViewer'
import { Badge } from '../components/ui/badge'

interface Skin {
  id: string
  name: string
  image: string
  modelPath: string
  available: boolean
}

export default function Agent() {
  const skins: Skin[] = [
    {
      id: 'player',
      name: 'Default',
      image: '/images/musk.png',
      modelPath: '/models/player.glb',
      available: true
    },
    {
      id: 'zucc',
      name: 'Zucc',
      image: '/images/zucc.png',
      modelPath: '/models/zucc.glb',
      available: true
    },
    {
      id: 'huang-1',
      name: 'Huang',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    },
    {
      id: 'huang-2',
      name: 'Huang Gold',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    },
    {
      id: 'huang-3',
      name: 'Huang Elite',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    },
    {
      id: 'huang-4',
      name: 'Huang Pro',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    },
    {
      id: 'huang-5',
      name: 'Huang Max',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    },
    {
      id: 'huang-6',
      name: 'Huang Ultra',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    },
    {
      id: 'huang-7',
      name: 'Huang Prime',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    },
    {
      id: 'huang-8',
      name: 'Huang Legend',
      image: '/images/huang.png',
      modelPath: '/models/huang.glb',
      available: false
    }
  ]

  const [selectedSkin, setSelectedSkin] = useState<Skin>(skins[0])

  return (
    <div className="h-full flex flex-col p-8 gap-8">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold text-foreground">Your Agent</h1>
        <p className="text-muted-foreground mt-2">
          Customize your character with different skins
        </p>
      </div>

      <div className="flex-1 grid grid-cols-1 lg:grid-cols-2 gap-8 min-h-0">
        {/* 3D Model Viewer */}
        <div className="flex flex-col">
          <h2 className="text-xl font-semibold mb-4 text-foreground">Preview</h2>
          <div className="flex-1 min-h-[400px] lg:min-h-0">
            <Model3DViewer modelPath={selectedSkin.modelPath} />
          </div>
          <div className="mt-4 text-center">
            <p className="text-sm text-muted-foreground">
              Drag to rotate • Scroll to zoom
            </p>
          </div>
        </div>

        {/* Skin Inventory */}
        <div className="flex flex-col">
          <div className="mb-4">
            <h2 className="text-xl font-semibold text-foreground">Inventory</h2>
            <p className="text-sm text-muted-foreground mt-1">
              Select a skin to equip
            </p>
          </div>

          {/* Available Skins */}
          <div className="mb-6">
            <h3 className="text-sm font-medium text-foreground mb-3">Available</h3>
            <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
              {skins.filter(skin => skin.available).map((skin) => (
                <button
                  key={skin.id}
                  onClick={() => setSelectedSkin(skin)}
                  className={`relative aspect-square rounded-xl overflow-hidden border-2 transition-all ${
                    selectedSkin.id === skin.id
                      ? 'border-primary shadow-lg scale-105'
                      : 'border-border hover:border-primary/50'
                  }`}
                >
                  <img
                    src={skin.image}
                    alt={skin.name}
                    className="w-full h-full object-cover"
                  />
                  {selectedSkin.id === skin.id && (
                    <div className="absolute top-2 right-2">
                      <Badge variant="default" className="bg-primary">
                        Equipped
                      </Badge>
                    </div>
                  )}
                  <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-3">
                    <p className="text-white text-sm font-medium">{skin.name}</p>
                  </div>
                </button>
              ))}
            </div>
          </div>

          {/* Coming Soon Skins */}
          <div>
            <h3 className="text-sm font-medium text-foreground mb-3 flex items-center gap-2">
              Coming Soon
              <Badge variant="secondary" className="text-xs">
                {skins.filter(skin => !skin.available).length}
              </Badge>
            </h3>
            <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
              {skins.filter(skin => !skin.available).map((skin) => (
                <div
                  key={skin.id}
                  className="relative aspect-square rounded-xl overflow-hidden border-2 border-border opacity-60 cursor-not-allowed"
                >
                  <img
                    src={skin.image}
                    alt={skin.name}
                    className="w-full h-full object-cover grayscale"
                  />
                  <div className="absolute inset-0 flex items-center justify-center bg-black/50">
                    <Badge variant="secondary" className="bg-muted/80">
                      LOCKED
                    </Badge>
                  </div>
                  <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-3">
                    <p className="text-white text-sm font-medium">{skin.name}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
