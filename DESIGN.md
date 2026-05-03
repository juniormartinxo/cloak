# Design

## Visual Theme

`cloak` uses a product-led interface structure while preserving the original black, off-white and red identity. The page has a connected dark stage with red/black gradient depth, a real top navigation surface, a centered software mockup joined to the CLI strip, red active states, and concrete CLI/profile panels. The site should feel like a focused local tool with a visible runtime, not disconnected sections floating in empty space.

## Color Palette

- `cloak-ink`: near-black with a warm red tint for the outer stage.
- `cloak-stage`: slightly warmer red-black used inside gradient depth fields.
- `cloak-shell`, `cloak-panel`, `cloak-card`: off-white and tinted paper surfaces for the product mockup and panels.
- `cloak-line`: muted red divider lines.
- `cloak-text`, `cloak-soft`, `cloak-muted`: dark neutral text roles for paper surfaces.
- `cloak-ember`: the original saturated red for active state, install CTA and warnings.
- `cloak-green` and `cloak-blue`: sparse semantic aliases mapped to the existing cyan accent.

All custom colors should be declared in Tailwind v4 `@theme` using OKLCH values.

## Typography

Use a committed system sans stack for display and body text, with generic monospace only for commands, file paths, env vars, and code specimens. Headings should be large, calm and product-focused, with uppercase reserved for short labels and status text. Body text should stay short and under 75 characters per line.

## Components

- Navigation bar as a real rounded product surface, not loose text on the background.
- Product mockup with sidebar profiles, active repo state, command output and execution steps, visually attached to the hero and CLI strip.
- Floating status pills for profile resolution, env cleanup and daemon-free operation.
- CLI compatibility strip using real supported tool names.
- Asymmetric feature panels with concrete commands and profile facts.
- Install and doctor sections rendered as terminal surfaces.

## Layout

The page is a long product surface. Use a connected hero shell that contains nav, headline, mockup and CLI strip first, then alternate wide explanatory sections with compact operational panels. Keep the mobile layout single-column, reduce floating labels, and preserve command readability.

## Motion

Use only subtle transitions for links and controls. Avoid layout animation, bounce, or decorative motion.
