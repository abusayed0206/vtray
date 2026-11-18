---
name: VT Tray
description: A pharmacy dispensing label and patient leaflet for a VirusTotal tray app.
colors:
  ground: "#e6e5dd"
  stock: "#f6f6f1"
  window: "#ffffff"
  ink: "#111111"
  ink-2: "#3b3b38"
  rule: "#c4c3ba"
  sticker-yellow: "#ffd400"
  yellow-ink: "#2b2400"
  sticker-orange: "#ff7a1a"
  orange-ink: "#2a1200"
  sticker-blue: "#8fd0ef"
  blue-ink: "#0c2533"
  verdict-green: "#17733f"
typography:
  display:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "clamp(38px, 4.4vw, 64px)"
    fontWeight: 800
    lineHeight: 0.98
    letterSpacing: "-0.01em"
    fontVariation: "'wdth' 68"
  headline:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "clamp(32px, 4vw, 52px)"
    fontWeight: 800
    lineHeight: 1
    fontVariation: "'wdth' 70"
  title:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "19px"
    fontWeight: 700
    lineHeight: 1.3
    fontVariation: "'wdth' 80"
  body:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.55
  body-large:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "19px"
    fontWeight: 400
    lineHeight: 1.55
  label:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 700
    letterSpacing: "0.08em"
    fontVariation: "'wdth' 75"
  nav:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    letterSpacing: "0.04em"
    fontVariation: "'wdth' 85"
  cta:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "26px"
    fontWeight: 800
    lineHeight: 1.15
    fontVariation: "'wdth' 72"
  value:
    fontFamily: "JetBrains Mono, ui-monospace, monospace"
    fontSize: "15px"
    fontWeight: 400
    fontFeature: "'tnum'"
rounded:
  control: "6px"
  screenshot: "8px"
  label: "10px"
  sticker: "12px"
spacing:
  gutter: "32px"
  gutter-phone: "16px"
  field: "12px 16px"
  panel: "28px"
  column-gap: "56px"
  section: "96px"
  section-phone: "72px"
components:
  button-take:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.cta}"
    rounded: "{rounded.control}"
    padding: "16px 18px"
  button-take-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.stock}"
  nav-download:
    textColor: "{colors.ink}"
    typography: "{typography.nav}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
  nav-download-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.stock}"
  dispensing-label:
    backgroundColor: "{colors.stock}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
    padding: "36px 44px 32px"
  label-field:
    textColor: "{colors.ink}"
    typography: "{typography.value}"
    padding: "{spacing.field}"
  sticker-yellow:
    backgroundColor: "{colors.sticker-yellow}"
    textColor: "{colors.yellow-ink}"
    rounded: "{rounded.sticker}"
    padding: "18px 20px"
    width: "230px"
  sticker-blue:
    backgroundColor: "{colors.sticker-blue}"
    textColor: "{colors.blue-ink}"
    rounded: "{rounded.sticker}"
    padding: "12px 16px"
    width: "210px"
  peel-sticker:
    backgroundColor: "{colors.sticker-orange}"
    textColor: "{colors.orange-ink}"
    rounded: "{rounded.label}"
    padding: "22px 24px 30px"
    width: "min(330px, 78%)"
  dialog:
    backgroundColor: "{colors.window}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
    padding: "22px 26px 20px"
  leaflet-panel:
    backgroundColor: "{colors.stock}"
    textColor: "{colors.ink}"
    padding: "{spacing.panel}"
---

# Design System: VT Tray

## Overview

**Creative North Star: "The Dispensing Label"**

Every file gets a pharmacy label. The site is printed matter laid on a counter: off-white thermal label stock on a warm grey ground, black ink, dense condensed caps for printed field names, a monospaced face for printed values, and die-cut auxiliary warning stickers whose saturated colours take over whole page regions. The download is the label's perforated tear-off stub, not a floating button.

Density is a label's density: hairline rules, tight fields, numbered small print. Depth is physical and modest, the slight lift of a label or sticker off a surface, never glassy or glowing. Motion is limited to two mechanical acts the material can perform: a label printer feeding lines, and a sticker peeling off. The confirmed anti-reference is the dark-navy shield security page and the split hero with a feature-card row.

**Key Characteristics:**
- Label stock (#f6f6f1) on counter ground (#e6e5dd); ink is near-black, never pure grey text on grey.
- One variable family (Archivo) carries every role through its width axis; JetBrains Mono carries values.
- Sticker colours (yellow, orange, pale blue) own whole full-bleed fields, each with its own dark tinted ink.
- Perforation dashes, die-cut notches and rounded corners are the component language.
- Real screenshots at their natural pixel scale are the only imagery.

## Colors

A printed-matter palette: two papers, one ink, three sticker stocks, and one verdict green.

### Primary
- **Thermal Ink** (ink): all printed type, the 3px header rules under label heads and section heads, button strokes, focus outlines, and the filled hover state of every download control.

### Secondary
- **Warning Yellow** (sticker-yellow): the "Ask before uploading" sticker and the full-bleed consent field. Also the text selection colour. Text on it is **Yellow Ink** (yellow-ink), never black.
- **Caution Orange** (sticker-orange): the peelable consent sticker and the full-bleed "Do not exceed" field. Text on it is **Orange Ink** (orange-ink).
- **Advisory Blue** (sticker-blue): the "Advisory only" sticker and the full-bleed side-effects field. Text on it is **Blue Ink** (blue-ink).

### Tertiary
- **Verdict Green** (verdict-green): the "0" in the dosage line's engine count and nothing else.

### Neutral
- **Counter Ground** (ground): page background; also fills the stub's die-cut notches so they read as holes.
- **Label Stock** (stock): every label, the leaflet panels, the dose strip; inverse text on ink hovers.
- **Window White** (window): only the reproduced Windows dialog, so the real app UI reads as a different material from the label.
- **Faded Ink** (ink-2): field names, captions, secondary small print, the label number.
- **Hairline** (rule): field dividers, perforation dashes, fold lines, FAQ separators.

### Named Rules
**The Whole Region Rule.** A sticker colour either is a small die-cut sticker or fills a full-bleed field edge to edge (yellow text selection is the one exception). It never appears as a tint, a border accent, a badge or a text colour.

**The Matched Ink Rule.** Each sticker stock has its own dark ink of the same hue (yellow-ink, orange-ink, blue-ink) for type and shadows on it; shadows under labels on a coloured field are tinted with that field's ink.

**The One Green Rule.** Verdict green marks a clean verdict figure only. It is not a success colour, link colour or button colour.

## Typography

**Display Font:** Archivo variable (weight 400-800, width 62-125%), with system-ui fallback
**Body Font:** Archivo at normal width
**Label/Mono Font:** JetBrains Mono (400-600), with ui-monospace fallback

**Character:** One grotesk squeezed to different widths does all the label work: narrow heavy caps for printed headings, medium-width caps for field names and nav, normal width for leaflet reading. The mono face is reserved for things a printer would stamp: hashes, sizes, counts, numbers.

### Hierarchy
- **Display** (800, width 68%, clamp(38px, 4.4vw, 64px), 0.98, uppercase, max 21ch): the label's directions line, the h1. 40px fixed under 520px.
- **Headline** (800, width 70%, clamp(32px, 4vw, 52px), 1, uppercase): section h2s in leaflet heads and coloured fields.
- **Title** (700, width 80%, 19px, 1.3): numbered leaflet part headings, preceded by a mono part number.
- **Body** (400, 17px, 1.55; 16px under 520px): leaflet small print, FAQ answers (max 64ch).
- **Body Large** (400, 19px): dosage line, consent copy (max 54ch), dose lines, orange field copy (max 62ch).
- **Label** (700, width 75%, 12px, 0.08em, uppercase): printed field names on labels.
- **Nav** (600, width 85%, 15px, 0.04em, uppercase): top bar links.
- **CTA** (800, width 72%, 26px, 1.15, uppercase): the take-off instruction inside the stub.
- **Value** (JetBrains Mono 400, 15px, tabular nums, wraps anywhere): field values; hashes at 14px with 0.02em; dose figures at 600/22px.
- Sticker headings run 800 at width 66%, 26px (18px on the blue sticker, 30px on the peel sticker), line-height 1, uppercase.

### Named Rules
**The Width Axis Rule.** Hierarchy comes from font-stretch and weight, not from new families: narrower and heavier means more "printed". Keep headings between 66% and 72% width, field names and nav between 75% and 85%, reading text at 100%.

**The Stamped Value Rule.** Anything a machine would print (hash, size, version, count, limit) is set in JetBrains Mono with tabular figures. Prose numbers in sentences are not.

## Layout

A single 1200px column (side gutter 32px, 16px under 520px) for the counter, leaflet, closing label and footer. Coloured fields break out full-bleed and keep their content aligned to the same column with `max(32px, calc((100vw - 1136px) / 2))` side padding. Sections are spaced 96px vertically (72px inside coloured fields under 520px).

- **Dispensing label:** a two-column grid, sheet plus a fixed 280px stub. Label fields sit in a four-column grid (1.7fr 0.7fr 1fr 1.3fr) with a full-width row for the hash.
- **Leaflet:** three equal panels on stock, separated by dashed fold lines.
- **Coloured fields:** consent is copy plus a 480px dialog column, side effects is two equal columns, both with a 56px gap; the orange field is a single column with the dose strip capped at 760px.
- **Pack:** screenshots sit in an auto-width grid at their intrinsic pixel size; only the oversized installer shot is capped (494px).

**At 960px and below:** labels collapse to one column and the stub moves under the sheet (perforation turns horizontal, notches move to its left and right ends); label fields go two-up; leaflet, consent and side-effects grids stack; the pack goes single column; the top bar keeps only the Download link; the blue sticker drops into flow after the stub so it covers no links.

**At 520px and below:** 16px gutters, body 16px, display 40px, label head wraps, stickers shrink (168px, 20px headings), leaflet panels pad 22px 18px, peel sticker widens to 86%.

### Named Rules
**The Natural Scale Rule.** App screenshots render at their native pixel size (width and height attributes match the file); they are never upscaled or cropped into decorative frames.

## Elevation & Depth

Depth is physical lift: soft, two-layer shadows that read as paper resting on a surface. A small tight contact shadow plus a wider diffuse one. Stickers sit higher than labels and are rotated a few degrees. Nothing glows, and no shadow is a hard offset.

### Shadow Vocabulary
- **Label lift** (`box-shadow: 0 1px 1px rgba(17,17,17,0.08), 0 10px 26px rgba(17,17,17,0.1)`): dispensing label and closing label on the ground.
- **Sticker lift** (`box-shadow: 0 2px 2px rgba(17,17,17,0.1), 0 8px 18px rgba(17,17,17,0.14)`): die-cut warning stickers.
- **Screenshot lift** (`box-shadow: 0 1px 2px rgba(17,17,17,0.12), 0 6px 16px rgba(17,17,17,0.1)`): pack screenshots.
- **Dialog on yellow** (`box-shadow: 0 2px 4px rgba(43,36,0,0.12), 0 16px 36px rgba(43,36,0,0.16)`): the window on the consent field.
- **Strip on orange** (`box-shadow: 0 2px 3px rgba(42,18,0,0.16), 0 12px 26px rgba(42,18,0,0.16)`): the dose strip.
- **Peel sticker** (`filter: drop-shadow(0 2px 2px rgba(42,18,0,0.2)) drop-shadow(0 10px 18px rgba(42,18,0,0.2))`): a filter, not box-shadow, so the shadow follows the clipped folded corner.

### Named Rules
**The Paper Lift Rule.** Every raised surface uses a contact layer under 4px plus a diffuse layer of 6-36px blur, at 0.08-0.2 alpha, tinted with the ink of the surface beneath it.

## Shapes

Die-cut forms. Labels and the dialog round at 10px, stickers at 12px, controls at 6px, screenshots at 8px. The perforation is a 2px dashed hairline-colour border with two 24px circular notches cut from the edges, filled with the ground colour. Leaflet folds are 1px dashed. Heavy 3px ink rules close every label head and section head; 1px rules divide fields and list items. Stickers rotate between -4deg and 2deg. The peel sticker's lifted corner is cut away with a `clip-path` polygon and backed by a cream liner gradient.

Markers are drawn in CSS at a 2px stroke: list ticks are a rotated L of borders, FAQ plus and minus are crossed linear gradients, the download arrow is an inline SVG at 2.5 stroke with round caps.

## Components

### Buttons
Printed instructions you could tear off, not pills.
- **Shape:** gently rounded rectangle (6px), 3px ink stroke on the stub take-off, 2px on the nav Download.
- **Take-off (primary):** transparent on stock, ink text; a CTA line with a drawn down-arrow and a mono sub-line (price, version, size) beneath, padding 16px 18px.
- **Hover / Focus:** fills solid ink with stock text over 160ms ease-out. Focus is the global 3px ink outline at 3px offset.
- **Nav Download:** same inversion on hover, 6px 12px padding, uppercase nav type.

### Dispensing Label (signature)
The hero and the closing card.
- **Corner Style:** 10px.
- **Background:** stock, with Label lift.
- **Structure:** a label head (heavy narrow brand caps, a faded kind line, a mono label number pushed right) closed by a 3px ink rule; then the display directions line; then a grid of fields, each a small uppercase name over a mono value, divided by hairlines.
- **Stub:** a fixed 280px column behind a dashed perforation with ground-coloured notches, holding the tear instruction (mono 12px, faded), the take-off button, a hairline-ruled fact list and plain text links. The closing label reprises the same stub at 760px max width.
- **Print feed:** on load the directions, each field and the dosage line reveal left to right with `clip-path: inset(0 100% 0 0)` to `inset(0)` over 520ms `cubic-bezier(0.16, 1, 0.3, 1)`, staggered 60ms from 120ms. Content is visible without JavaScript; the feed only runs under a `.js` class.

### Warning Stickers
- **Style:** solid sticker stock, matched ink, 12px corners, Sticker lift, slight rotation, absolutely placed to overlap a label edge.
- **Content:** one narrow heavy caps line plus one 13px line of small print.
- **Variants:** yellow (consent warning, 230px, -4deg) and blue (advisory, 210px, 2deg).

### Peel Sticker (signature)
An orange sticker laid over the real consent dialog, one corner already lifted.
- **Shape:** 10px corners, clipped by a polygon whose cut corner size is the `--fold` custom property (46px at rest), with a liner-side triangle painted in the cut.
- **Motion:** `--fold` is registered with `@property` as `<length-percentage>`, so it animates; clicking peels it to 160% over 700ms `cubic-bezier(0.16, 1, 0.3, 1)`, then it fades out (240ms, 520ms delay) and stops taking pointer events.
- **Behaviour:** a real `<button>` with `aria-controls` and `aria-expanded`; the dialog is `aria-hidden` until peeled, then receives focus. Without JavaScript the sticker is not rendered and the dialog shows.

### Dialog
- **Style:** window white, 10px corners, yellow-tinted lift, 15px text with a 17px bold title and mono file facts; Yes and No as 6px outlined chips, No with a 2px ink stroke as the default.

### Leaflet Panels
- **Style:** three panels on stock under a section head, 28px padding, dashed fold lines between, bottom corners rounded 6px. Each heading leads with a mono part number. Screenshots inside sit at natural scale with an 8px radius and a faded 13px caption.

### Dose Strip
- **Style:** a stock label on the orange field, 10px corners, a narrow-caps head over a 3px ink rule, then hairline-ruled lines whose limits are mono 600 at 22px.

### Navigation
- **Style:** a plain top bar on the ground, mark at left (800, width 75%, 22px caps), uppercase nav links at right underlined on hover, the Download link outlined. Under 960px only Download remains.

### FAQ
- **Style:** native `details`, hairline-separated, 700 width-85% 20px questions, a drawn 14px plus that becomes a minus when open, faded answers at 64ch.

## Do's and Don'ts

### Do:
- **Do** put new content on label stock (#f6f6f1) laid on the counter ground (#e6e5dd), closed at the top by a label head with a 3px ink rule.
- **Do** let a sticker colour own a full-bleed field, with its matched ink for all text on it.
- **Do** set machine-printed values (hashes, sizes, counts, limits, versions) in JetBrains Mono with tabular figures.
- **Do** build hierarchy with Archivo's width axis: 66-72% for headings, 75-85% for field names and nav.
- **Do** make the download a stub behind a dashed perforation with die-cut notches.
- **Do** draw markers (ticks, plus/minus, arrows) in CSS or inline SVG at a 2px stroke.
- **Do** show real screenshots at their natural pixel size.
- **Do** keep motion to printer feed and sticker peel, and render everything static under `prefers-reduced-motion`.

### Don't:
- **Don't** build the dark-navy shield security page or a split hero with a feature-card row.
- **Don't** use sticker colours as tints, borders, badges or text colours.
- **Don't** use verdict green for anything but a clean verdict figure.
- **Don't** add a second sans family; the width axis does that work.
- **Don't** use hard offset shadows, glows or glass; depth is paper lift only.
- **Don't** put small uppercase eyebrow labels above section headings; the label head and the tear instruction on the stub are the only printed captions.
