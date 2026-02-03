# Character System

Game characters with distinct personalities, backstories, and behavioral profiles.

## Characters

| Character | Role | Chaos Level |
|-----------|------|-------------|
| [Clawrence](./clawrence.md) | The Unhinged AI Lobster | Maximum |

## Structure

```
/characters/
├── README.md           # This file
├── <character>.md      # Character profile
└── models/
    └── <character>.glb # 3D model
```

## Character Profile Format

Each character file includes:
- **Core Identity** - Who they are, why they exist
- **Catchphrases** - Quotable lines
- **Personality Traits** - Behavioral description
- **Backstory** - Shareable lore
- **Behavior Profile** - Numeric stats for AI/gameplay

## Models

3D models are stored in GLB format in the `models/` directory, ready for import into the game engine.
