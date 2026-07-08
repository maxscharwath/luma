/** Layered backdrop + scrims for the cinematic `DetailHero` (movie/series fiche).
 *
 * The hero overlays text on an *unknown* key-art image, so legibility can't rely
 * on the artwork being dark but the artwork should still read through as much
 * as possible. The layers are tuned art-forward: large transparent zones reveal
 * the backdrop, and every dark edge fades over a long, soft distance so there's
 * no visible seam. Text legibility is carried by the gradients near the left
 * gutter *plus* the title halo / column text-shadow defined in `DetailHero`.
 *
 *  1. backdrop image (bg-cover) the raw, un-dimmed art.
 *  2. radial falloff a big transparent core keeps most of the art visible;
 *     only the far edges sink into the page bg.
 *  3. left scrim anchored dark at the very left (under the start of the text),
 *     then a long gentle fade so the image reveals across the hero.
 *  4. bottom fade clean hand-off to the page below.
 *  5. reading frost a *light* `backdrop-blur` + dark fill behind the text
 *     column only, wide-feathered via a left-anchored mask so the art shows
 *     through and it blends into the image rather than reading as a card. The
 *     mask is in `rem` (tracks layout / font-scaling, not viewport width).
 */
export function HeroBackdrop({ bg }: Readonly<{ bg: string }>) {
  return (
    <>
      <div className="absolute inset-0 bg-cover bg-center" style={{ backgroundImage: bg }} />
      <div className="absolute inset-0 bg-[radial-gradient(125%_125%_at_80%_22%,transparent_38%,var(--luma-bg)_94%)]" />
      <div className="absolute inset-0 bg-[linear-gradient(90deg,var(--luma-bg)_0%,rgba(10,10,12,.74)_22%,rgba(10,10,12,.34)_46%,rgba(10,10,12,.08)_64%,transparent_80%)]" />
      <div className="absolute inset-0 bg-[linear-gradient(0deg,var(--luma-bg)_3%,transparent_46%)]" />
      {/* Reading frost light + wide so the backdrop still reads through. */}
      <div
        className="absolute inset-0 backdrop-blur-[2px]
          bg-[linear-gradient(to_top,rgba(10,10,12,.58)_0%,rgba(10,10,12,.34)_100%)]
          [mask-image:linear-gradient(90deg,#000_0rem,#000_22rem,transparent_68rem)]"
      />
    </>
  );
}
