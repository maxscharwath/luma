<div align="center">
  <img src="../../.github/assets/logo.svg" alt="KROMA" height="56">
  <h1>@kroma/ui</h1>
  <p><i>KROMA design-system React components + design tokens.</i></p>
</div>

> Part of the [KROMA](../../README.md) monorepo. The presentation layer shared by
> every client components and CSS tokens ported from the
> [design source](../../design/readme.md) (deep-charcoal + amber, Bricolage
> Grotesque / Hanken Grotesk, no emoji).

## Usage

```tsx
import '@kroma/ui/styles.css';            // tokens + base styles (once, at the root)
import { Button, Badge, Chip, Avatar, PosterCard, Logo } from '@kroma/ui';

<Logo size={28} />
<PosterCard title="Blade Runner 2049" year={2017} badge="4K HDR" />
<Button variant="primary">Lecture</Button>
<Badge tone="quality">H.265</Badge>
```

`react` / `react-dom` are **peer dependencies** (≥ 18). Components are consumed as
source through the workspace no build step.

## Components

| Component | Notes |
| --------- | ----- |
| `Button` | `variant` (`primary` / `ghost` / …) × `size`; amber primary, springy press. |
| `Badge` | Status & quality pills (`tone`: quality / status / accent…). Text codes `4K`, `HDR`, `H.265`, `5.1` never icons. |
| `Chip` | Compact metadata / filter pill (999px radius). |
| `Avatar` | Profile avatar (initials or image). |
| `PosterCard` | The workhorse poster tile lazy `<img loading="lazy" decoding="async">`, `content-visibility`, `React.memo`'d, bottom scrim for legible titles, hover-lift + amber ring. |
| `Logo` | The KROMA brand lockup an amber "aperture" ring + centre dot beside the wordmark (`markOnly` for just the mark). |

## Design tokens

`@kroma/ui/styles.css` exposes the brand as CSS custom properties, so app code and
TV/web shells stay on-brand without re-declaring values:

```css
--kroma-bg: #0A0A0C;        /* deep charcoal page */
--kroma-text: #F4F3F0;
--kroma-accent: #F4B642;    /* the single warm amber accent */
--font-display: 'Bricolage Grotesque', …;   /* cinematic titles */
--font-body:    'Hanken Grotesk', …;        /* UI / body */
/* + radius, shadow, easing tokens see design/tokens/ */
```

## Performance

Every visual primitive is tuned for weak TV hardware (and benefits the web too):
lazy/async poster decoding, `content-visibility: auto` to skip off-screen layout,
memoized tiles, and GPU-only (`transform` / `box-shadow`) focus animation for a
smooth 60 fps highlight.

## See also

- [`@kroma/core`](../core/README.md) the types & logic these components render
- [`@kroma/tv`](../tv/README.md) the 10-foot experience composed from these
- [design/readme.md](../../design/readme.md) the full design language
