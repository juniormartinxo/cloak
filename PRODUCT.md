# Product

## Register

brand

## Users

Developers and technical leads who use multiple LLM CLIs across work, personal, and client repositories. They need to move between directories without leaking credentials, mixing account state, or maintaining shell wrappers.

## Product Purpose

`cloak` isolates LLM CLI profiles per directory. It resolves the nearest `.cloak` file, prepares the correct config home and environment variables, removes conflicting secrets, and hands off to the real CLI with a clean `exec`. Success means the right account is active automatically for every repo.

## Brand Personality

Precise, guarded, direct. The brand should feel like a small security instrument: mechanical enough for CLI users, bold enough to make credential isolation memorable, and clear enough to explain the routing model in one pass.

## Anti-references

Avoid generic SaaS hero pages, soft pastel dashboards, glass panels, vague AI automation language, and decorative security theater. The interface should not hide behind stock photos or abstract gradients.

## Design Principles

- Make isolation visible: show profiles, directories, env cleanup, and the final CLI handoff as concrete system parts.
- Keep the CLI as the product: commands and file paths are primary artifacts, not supporting decoration.
- Be bold without becoming noisy: use strong typography and red structural marks, but keep the reading path disciplined.
- Explain by diagram: prefer routed flows, grids, and command specimens over paragraphs.
- Preserve trust: no fake dashboards, no inflated metrics, no claims beyond what the tool does locally.

## Accessibility & Inclusion

Target WCAG AA contrast, keyboard-readable content order, clear focus states, and reduced reliance on color alone. Motion should be nonessential and safe for reduced-motion users.
