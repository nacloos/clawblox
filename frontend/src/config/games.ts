export interface FeaturedGame {
  id: string
  name: string
  description: string
  gameType: string      // Maps to DB game_type
  assetName: string     // Maps to /public/games/{assetName}.png|mp4
  isActive: boolean     // true = clickable, false = coming soon
  order: number
}

export const FEATURED_GAMES: FeaturedGame[] = [
  {
    id: 'tsunami',
    name: 'Escape Tsunami For Brainrots',
    description: 'Collect brainrots, deposit for money, buy speed upgrades.',
    gameType: 'tsunami',
    assetName: 'tsunami',
    isActive: true,
    order: 1
  },
  {
    id: 'arsenal',
    name: 'Block Arsenal',
    description: 'Gun Game / Arms Race - Progress through 15 weapons.',
    gameType: 'lua',
    assetName: 'arsenal',
    isActive: false,  // Coming soon
    order: 2
  },
  {
    id: 'moba',
    name: 'MOBA Arena',
    description: 'Team battles with unique champions and abilities.',
    gameType: 'lua',
    assetName: 'moba',
    isActive: false,  // Coming soon
    order: 3
  },
  {
    id: 'brawl',
    name: 'Brawl Stars',
    description: 'Fast-paced 3v3 team battles and showdowns.',
    gameType: 'lua',
    assetName: 'brawl',
    isActive: false,  // Coming soon
    order: 4
  }
]

export function getGameAsset(assetName: string, type: 'image' | 'video'): string {
  if (type === 'image') {
    return `/games/${assetName}-wallpaper-600x800.png`
  }
  return `/games/${assetName}.mp4`
}

export function getFeaturedGameByType(gameType: string): FeaturedGame | undefined {
  return FEATURED_GAMES.find(g => g.gameType === gameType)
}

export function getFeaturedGameById(id: string): FeaturedGame | undefined {
  return FEATURED_GAMES.find(g => g.id === id)
}
