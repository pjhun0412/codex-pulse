# Prototype Instructions

Run the local server yourself and open the preview in the browser available to this environment. Do not give the user server-start instructions when you can run it.

Before making substantial visual changes, use the Product Design plugin's `get-context` skill when the visual source is unclear or no longer matches the current goal. When the user gives durable prototype-specific design feedback, preferences, or decisions, record them in `AGENTS.md`.

When implementing from a selected generated mock, treat that image as the source of truth for layout, component anatomy, density, spacing, color, typography, visible content, and hierarchy.

## Codex Pulse design decisions

- The selected visual target is the third generated concept: a compact 2×2 resource matrix in the Windows taskbar with an attached, data-dense detail panel.
- Keep the interface credible as a real Windows developer utility. Avoid stereotypical AI-generated styling: no excessive glow, card grids, gratuitous gradients, oversized headings, decorative copy, or inconsistent spacing.
- Codex is the primary signal. CPU and memory are supporting metrics.
- Use precise Segoe UI typography, restrained mint/amber/violet status colors, hairline dividers, and flat dark surfaces.
- Never invent a subscription expiration date. Show only fields returned by Codex app-server; rate-limit reset times are allowed.
- Never show fabricated usage or system metrics. Unavailable values must render as `-` with the actual connection error.
- Support two placement modes: a Win32 child window embedded under Explorer's taskbar window, and a draggable free overlay.
- Taskbar mode must follow the taskbar's stacking and visibility. Do not force the widget above fullscreen applications or remote desktop windows; use a real `WS_CHILD` parent relationship instead of a topmost popup.
- Keep the native taskbar widget compact at 248 logical pixels wide. Preserve both metric columns with tighter padding, and keep its background stable while menus or detail panels cover nearby screen pixels.
- Keep the compact bar and detail popup as separate native windows. Do not resize one transparent window to toggle details.
- Close the detail popup only after both native windows lose focus, so clicks between the compact bar and detail panel remain usable.

Build app UI in `src/`. Keep `.openai/hosting.json`, `worker/index.js`, `scripts/prepare-sites-build.mjs`, and `tests/sites-worker.test.mjs` intact so the same local prototype can be handed to Sites. Before a Sites handoff, run `npm run build` and `npm run test:sites`; the build must leave `dist/client/index.html`, `dist/server/index.js`, and `dist/.openai/hosting.json`.
