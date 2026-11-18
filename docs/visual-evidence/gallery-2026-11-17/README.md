# Gallery visual failure of 2026-11-17: evidence and root cause

Test: `tests/browser/visual.spec.mjs`, "scientific plot visual fixtures match their published gallery cards".
Failing cards: `bars` (height 362 px against a 361 px baseline), `scatter-fit` (height 362 against 361) and `heatmap`
(same size, 4933 pixels different, all of them on the contour lines and the text).

## Environment

| Item | Value |
|---|---|
| Operating system | Windows 10 Pro, build 10.0.26200 |
| Playwright | `@playwright/test` 1.63.0 |
| Chromium | 153.0.8010.12 (the version the bundled Playwright launches) |
| Viewport | 1440 x 960 (the test's desktop profile) |
| deviceScaleFactor | 1 (the Playwright default; not set in the configuration) |
| Figure font stack | `Inter, Segoe UI, Helvetica, Arial`. This machine has Segoe UI and Arial and does not have Inter, so Segoe UI is used. |
| Snapshot platform | `chromium-win32` (this machine); the Linux baselines are not run here |

## Reproduction

The failure is deterministic on this machine: three consecutive runs of the scientific-plot test gave the same failing
set (`bars`, `scatter-fit`, `heatmap`), and the captured actual images have identical bytes in runs 3 and 4
(`plot-bars-actual`, `plot-scatter-fit-actual`, `plot-heatmap-actual`: SHA-256 equal). Files:
`failing-run/playwright-log-run3.txt`, `failing-run/playwright-log-run4.txt`, `failing-run/*-actual.png`,
`failing-run/*-expected.png`, `failing-run/*-diff.png`.

## Content is not the cause

The published SVGs changed on 2026-11-08 (commits `8619758e` and `2df39c1f`, clipPath and gradient identifiers). The
previous versions were rendered as standalone `<img>` elements at the same width and position, in the same browser:
the PNG files are byte-identical (`svg-comparison/`). Compared line by line, the two versions differ only in the generated `clipPath` identifiers (4 lines in `scatter-fit`, 8 in `heatmap`).

## Layout position is the cause

A standalone `<img>` of the current `scatter-fit.svg` (577 px wide, natural height 360.625 px) changes its rendered
height and pixels with its fractional vertical position:

| Vertical offset of the image | scatter-fit rendered height | scatter-fit matches the baseline | heatmap rendered height | heatmap matches the baseline |
|---|---|---|---|---|
| 0 | 361 | yes | 481 | no |
| 0.25 | 361 | yes | 482 | no (1073 px differ) |
| 0.5 | 362 | no | 482 | yes |
| 0.75 | 362 | no | 482 | no (1187 px differ) |

(The baseline heights are 361 for `scatter-fit` and 482 for `heatmap` on Windows.) The probe is a diagnostic that was
run and removed; the table is its result.

In the gallery the cards sit at fractional positions that depend on the whole page above them. The Windows baselines
were recorded on 2026-10-25 (`34b20d63`). Since then the gallery layout changed several times: polished card styles
(`7fe6e7a0`, 2026-11-06), three figures added (`e995e8c1`, 2026-11-07), and card fixes (`a1757e89`, 2026-11-03). The
project refreshed the Linux baselines after the polished layout (`a4f8928e`, 2026-11-06) and the Linux heatmap (`4b4e380d`,
2026-11-07), but the Windows baselines were never refreshed. The stale Windows references are therefore the root cause.

## Decision and evidence of the correction

The three Windows baselines were replaced with the images the test captured on this machine (`after-baselines/`). Before
replacing them, the captured images were checked: the SVG content is unchanged (identical bytes at the same position),
the layout change is documented above, and the capture is deterministic (identical bytes across runs). Nothing else was
changed: the other twelve baselines, the thresholds (`threshold` 0.3 and `maxDiffPixelRatio` 0.015) and the test logic
are the same. After the replacement, `visual.spec.mjs` passed three consecutive runs (`final-visual-*.log`).

## Limits

- The baselines are position-dependent. Any future change to the gallery layout above a card will move its fractional
  position and fail the test again; the remedy is the same refresh, with this evidence repeated.
- The Linux baselines were not checked on this machine (no Linux Playwright environment here).
- A stricter alternative (aligning each image to whole pixels in the test) would test a different thing from the page a
  reader sees, so it was not adopted.
