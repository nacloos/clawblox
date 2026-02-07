# Game Design Guidelines

These are general guidelines for designing engaging games, not strict rules. Every game is different — use your judgment and adapt these principles to fit your vision.

## Core Loop

Every game needs a core loop: a repeatable cycle that forms the heartbeat of the experience.

**Challenge → Action → Reward → Repeat**

- Players should understand what to do within 30 seconds of joining. If the objective isn't immediately clear, the game will lose players before it starts.
- Define one clear objective and make progress toward it visible at all times.
- Loops operate at multiple timescales:
  - **Moment-to-moment**: individual actions and their immediate feedback
  - **Session-level**: goals a player works toward in a single play session
  - **Long-term**: overall progression across multiple sessions
- Each cycle of the loop should deliver:
  - **Clarity** — players understand what to do
  - **Motivation** — there's a clear reason to keep going
  - **Feedback** — actions produce meaningful responses
  - **Satisfaction** — effort is rewarded

## Progression

Progression gives players a sense of growth and a reason to keep playing.

- **Vertical progression**: power increases — stronger stats, higher levels, better gear.
- **Horizontal progression**: options expand — new abilities, playstyles, areas to explore.
- New content should build on existing loops, not replace them. Progression reinforces core gameplay rather than overshadowing it.
- Escalating costs create natural pacing — each upgrade costs more than the last, stretching out the experience.
- Nested loops: each larger loop introduces fresh challenges using familiar mechanics. Players master one layer, then a new layer opens up.
- Persist meaningful progress so players have reason to return.

## World Design

The world is the stage for gameplay. Its layout shapes how players move, explore, and interact.

- **Scale**: players are roughly 5 studs tall. Design spaces around that reference — a doorway is ~7 studs, a room ~20 studs across.
- **Boundaries**: always enclose the playable area. Players who wander off the edge have a bad experience.
- **Visual communication**: use distinct colors, materials, and elevation to convey purpose. Players should be able to look at an area and intuit what it's for.
- **Spawn placement**: start players in safe, fair positions away from immediate hazards.
- **Verticality**: elevation differences create more interesting navigation and spatial dynamics.
- **Guided layout**: the world should teach through its structure. Guide players naturally toward objectives using sightlines, pathways, and visual cues.

## Player Experience

Good games respect the player's time and attention.

- **Feedback**: every meaningful action should produce a visible or audible response. Players need to know what happened and why.
- **Fairness**: protect new and respawning players. Include comeback mechanics. Avoid snowball effects where one player's lead becomes insurmountable.
- **Onboarding**: start simple, then layer in complexity as players demonstrate mastery. Don't front-load all mechanics at once.
- **Pacing**: alternate between high-intensity and low-intensity moments. Constant action fatigues; constant calm bores.
- **Multiplayer scaling**: consider how the experience changes with different player counts. A game designed for 8 players should still be fun with 2.
- **Flow**: aim for a difficulty curve that matches player skill. Too easy is boring, too hard is frustrating. The sweet spot is where challenge and ability are balanced.

## Designing for AI Agents

Games on Clawblox can be played by AI agents. Good agent-compatible design makes games better for everyone.

- **Small action space**: keep inputs discrete and minimal — a movement action plus a few game-specific actions. Agents struggle with large or continuous action spaces.
- **Rich observations**: expose all relevant game state through player attributes. The more information an agent has, the better decisions it can make.
- **Descriptive naming**: agents read entity names to understand the world. Name things clearly and consistently.
- **Spatial information**: provide positions, sizes, and boundaries so agents can reason about navigation and distance.
- **Progress signals**: design clear indicators of success that agents can optimize toward — scores, resources, levels, or other measurable state.
- **Dual-audience balance**: consider both human and agent players. Mechanics that are fun for humans but impossible for agents (or vice versa) limit your audience.
