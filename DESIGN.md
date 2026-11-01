# Design Direction

Documented from product owner decisions. The agent articulates; the direction belongs to the owner.

## Identity

Daily working tool for **solo developers** who open DevKu multiple times a day between coding sessions. Not designed for large teams, and not for beginners installing PHP for the first time. Priorities: fast and unobtrusive. Open, check status, run, close.

## Nature

**Dense control panel.** Multiple streams of information visible simultaneously: site list, PHP versions, ports, service statuses, real-time access logs. Dense does not mean cluttered; density is achieved through aligned columns and concise labels, not stacked cards.

Visual references: VS Code and Docker Desktop, not Linear and not SaaS marketing pages.

## Dial

```
ENERGY 1 / RHYTHM 2 / MOTION 1
```

- **ENERGY 1**: The tool does not greet you. No hero section, no illustrations, no marketing slogans.
- **RHYTHM 2**: One consistent frame, with a deliberate seam between the site list and the detail/settings panels.
- **MOTION 1**: Only hover effects and status transitions. No enter animations, no parallax, no pulsing spinners.

## Palette

Follows system theme preference. Both modes must be tested, never just one.

| Role | Light | Dark |
|---|---|---|
| Background | `#FAFAF9` | `#0C0D0F` |
| Surface (rows, cards, panels) | `#FFFFFF` | `#141619` |
| Border | `#E4E4E7` | `#26292E` |
| Primary text | `#18181B` | `#E7E7EA` |
| Secondary text | `#52525B` | `#A1A1AA` |
| Accent | `#B45309` | `#F59E0B` |
| Text on accent | `#FFFFFF` | `#1C1917` |

**Amber Accent Rules:**

- Amber is the sole accent color. No secondary colors for decoration.
- In light mode, amber is used for button fills and active badges, not text over light surfaces. For text, use `#B45309`.
- Process statuses use semantic colors, not amber: online (green), stopped (gray), failed (red).

## Density

- Single-line site rows (`44px` height).
- Clean borders (`1px solid var(--line)`) as section dividers, without decorative divider lines within groups.
- Fixed column widths rather than percentages so the eye can scan vertically.

## Identity Motif

A row-height status bar along the left edge of each site row, colored according to real status: green (ready), gray (stopped), red (failed). This indicator conveys actionable status, not decoration. Elements without live status do not receive status bars.

## Mandatory States

Every data view must explicitly implement all four states:

1. **Empty**: No items yet. State the next actionable step clearly (avoid bare "No data").
2. **Loading**: While scanning directories, querying runtimes, or waiting for processes.
3. **Error**: Display the root cause and the corrective action.
4. **Ready**: Normal operating state with live data.

## Explicit Restrictions

These exclusions exist because the design direction does not need them:

- Gradients as primary backgrounds, glassmorphism, glowing borders, or stacked drop shadows.
- Stacked card layouts for site lists; flat dense lists are faster to scan.
- Decorative icons without clear functional purpose.
- Fabricated numbers or metrics. If real data is not yet available, show an empty state.
- Continuous endless animations.
