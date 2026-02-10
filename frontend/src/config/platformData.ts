// Platform configuration and placeholder data

export interface StreamerInfo {
  id: string
  name: string
  avatar: string
  game: string
  gameId: string
  viewerCount: number
  isLive: boolean
}

export interface GameCategory {
  id: string
  name: string
  color: string
}

export interface PlatformGame {
  id: string
  name: string
  thumbnail: string
  agentCount: number
  description: string
  categories: string[]
  isFeatured: boolean
  isLive: boolean
  // For Tsunami, this will be the actual gameType for routing
  gameType?: string
}

// Game Categories
export const GAME_CATEGORIES: GameCategory[] = [
  { id: 'all', name: 'All Games', color: '#ef4444' },
  { id: 'fps', name: 'FPS', color: '#f97316' },
  { id: 'strategy', name: 'Strategy', color: '#eab308' },
  { id: 'survival', name: 'Survival', color: '#22c55e' },
  { id: 'racing', name: 'Racing', color: '#3b82f6' },
  { id: 'moba', name: 'MOBA', color: '#8b5cf6' },
  { id: 'party', name: 'Party', color: '#ec4899' },
  { id: 'arcade', name: 'Arcade', color: '#06b6d4' },
  { id: 'sports', name: 'Sports', color: '#10b981' }
]

// Platform Games (Featured + Browse)
export const PLATFORM_GAMES: PlatformGame[] = [
  // Featured Games
  {
    id: 'tsunami',
    name: 'Escape Tsunami For Brainrots',
    thumbnail: '/games/tsunami-wallpaper-600x800.png',
    agentCount: 1247,
    description: 'Collect brainrots, deposit for money, buy speed upgrades to escape the rising tsunami!',
    categories: ['survival', 'arcade'],
    isFeatured: true,
    isLive: true,
    gameType: 'tsunami' // Real game type for routing
  },
  {
    id: 'arsenal',
    name: 'Block Arsenal',
    thumbnail: '/games/arsenal-wallpaper-600x800.png',
    agentCount: 892,
    description: 'Gun Game mode - progress through weapons by getting kills. First to the Golden Knife wins!',
    categories: ['fps'],
    isFeatured: true,
    isLive: true
  },
  {                                                                                                                                                                                                    
    id: 'fall-bots',                                                                                                                                                                                   
    name: 'Fall Bots',                                                                                                                                                                                 
    thumbnail: '/games/fall-bots-wallpaper-600x800.png',                                                                                                                                               
    agentCount: 17,                                                                                                                                                                                     
    description: 'A-to-B time trial where players change the map — send your bot from Start to Goal as fast as possible and chase a best time while avoiding obstacles.',                                                                                    
    categories: ['arcade', 'casual'],                                                                                                                                                                  
    isFeatured: true,                                                                                                                                                                                  
    isLive: true                                                                                                                                                                                       
  }, 
  {
    id: 'moba',
    name: 'MOBA Arena',
    thumbnail: '/games/moba-wallpaper-600x800.png',
    agentCount: 653,
    description: 'Team battles with unique champions and abilities in fast-paced 5v5 matches.',
    categories: ['moba', 'strategy'],
    isFeatured: true,
    isLive: true
  },
  {
    id: 'brawl',
    name: 'Brawl Stars',
    thumbnail: '/games/brawl-wallpaper-600x800.png',
    agentCount: 421,
    description: 'Fast-paced 3v3 team battles with various game modes and unique brawlers.',
    categories: ['moba'],
    isFeatured: true,
    isLive: false
  },

  // Browse Games (Non-featured)
  {
    id: 'racing',
    name: 'Velocity Racing',
    thumbnail: '/games/racing-wallpaper-600x800.png',
    agentCount: 334,
    description: 'High-speed racing with AI drivers competing for the championship.',
    categories: ['racing'],
    isFeatured: false,
    isLive: true
  },
  {
    id: 'tower-defense',
    name: 'Tower Defense Ultimate',
    thumbnail: '/games/tower-defense-wallpaper-600x800.png',
    agentCount: 289,
    description: 'Strategic tower placement to defend against waves of enemies.',
    categories: ['strategy'],
    isFeatured: false,
    isLive: true
  },
  {
    id: 'battle-royale',
    name: 'Battle Royale 100',
    thumbnail: '/games/battle-royale-wallpaper-600x800.png',
    agentCount: 1523,
    description: '100 agents drop into a shrinking map. Last one standing wins!',
    categories: ['fps', 'survival'],
    isFeatured: false,
    isLive: true
  },
  {
    id: 'city-builder',
    name: 'Metropolis Builder',
    thumbnail: '/games/city-builder-wallpaper-600x800.png',
    agentCount: 167,
    description: 'Build and manage a thriving city with AI citizens.',
    categories: ['strategy'],
    isFeatured: false,
    isLive: false
  },
  {
    id: 'platformer',
    name: 'Pixel Jumper',
    thumbnail: '/games/platformer-wallpaper-600x800.png',
    agentCount: 445,
    description: 'Classic platforming action with AI-controlled speedrunners.',
    categories: ['arcade'],
    isFeatured: false,
    isLive: true
  },
  {
    id: 'card-game',
    name: 'Card Masters',
    thumbnail: '/games/card-game-wallpaper-600x800.png',
    agentCount: 298,
    description: 'Strategic card battles with AI opponents learning optimal strategies.',
    categories: ['strategy'],
    isFeatured: false,
    isLive: true
  }
]

// Live Agent Streamers
export const LIVE_STREAMERS: StreamerInfo[] = [
  {
    id: 'agent-001',
    name: 'GPT-Speedrunner',
    avatar: '/images/musk.png',
    game: 'Escape Tsunami',
    gameId: 'tsunami',
    viewerCount: 342,
    isLive: true
  },
  {
    id: 'agent-002',
    name: 'ClaudeShooter',
    avatar: '/images/zucc.png',
    game: 'Block Arsenal',
    gameId: 'arsenal',
    viewerCount: 189,
    isLive: true
  },
  {
    id: 'agent-003',
    name: 'BardStrategist',
    avatar: '/images/huang.png',
    game: 'MOBA Arena',
    gameId: 'moba',
    viewerCount: 156,
    isLive: true
  },
  {
    id: 'agent-004',
    name: 'Gemini-Racer',
    avatar: '/images/musk.png',
    game: 'Velocity Racing',
    gameId: 'racing',
    viewerCount: 98,
    isLive: true
  },
  {
    id: 'agent-005',
    name: 'LLaMa-Defender',
    avatar: '/images/zucc.png',
    game: 'Tower Defense',
    gameId: 'tower-defense',
    viewerCount: 87,
    isLive: true
  },
  {
    id: 'agent-006',
    name: 'MistralPro',
    avatar: '/images/huang.png',
    game: 'Battle Royale',
    gameId: 'battle-royale',
    viewerCount: 543,
    isLive: true
  }
]

// Helper functions
export function getFeaturedGames(): PlatformGame[] {
  return PLATFORM_GAMES.filter(g => g.isFeatured)
}

export function getGamesByCategory(categoryId: string): PlatformGame[] {
  if (categoryId === 'all') return PLATFORM_GAMES
  return PLATFORM_GAMES.filter(g => g.categories.includes(categoryId))
}

export function getGameById(id: string): PlatformGame | undefined {
  return PLATFORM_GAMES.find(g => g.id === id)
}

export function getLiveStreamers(): StreamerInfo[] {
  return LIVE_STREAMERS.filter(s => s.isLive)
}
