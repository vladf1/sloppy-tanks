"""The page's fonts in one collection (`public/fonts/interface.ttc`): Anton widened to
Impact's width for the display type, then Roboto Regular and Bold for the UI face, the
order `crates/desktop/src/ui/theme.rs` takes them in. One file is one download, and
Brotli finds what the Roboto weights share once they are in one stream (12% smaller
than the three files apart).

The page drew its headings in Impact, which may not be redistributed; the egui page draws
Anton (SIL OFL 1.1) in its place. Anton is a condensed face: drawn at the shell's
`ANTON_SCALE`, its capitals run 11.4% narrower than Impact's over the 1,095 letters of
the interface's uppercase strings (advance widths, Impact as macOS ships it), and its
space is a third wider. This stretches every outline, advance and kerning value along x
by `WIDTH` and gives the space Impact's advance, so headings take production's width.
Hinting is dropped: its programs were written for the unstretched outlines.

Run with fontTools:
    uv run --with fonttools python scripts/build-fonts.py
"""

from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.ttLib.ttCollection import TTCollection

ROOT = Path(__file__).resolve().parent.parent
SOURCES = ROOT / "assets/fonts"
TARGET = ROOT / "public/fonts/interface.ttc"

# Impact's letters over Anton's at the shell's scale (above), rounded.
WIDTH = 1.11
# Impact's space, 361/2048 em, in Anton's em at the shell's `ANTON_SCALE` (0.975).
SPACE_EM = 361 / 2048 / 0.975
FAMILY = "Anton Wide"


def widen_outlines(font: TTFont) -> None:
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    for name in font.getGlyphOrder():
        glyph = glyf[name]
        if glyph.isComposite():
            for component in glyph.components:
                component.x = round(component.x * WIDTH)
        elif glyph.numberOfContours > 0:
            glyph.coordinates.scale((WIDTH, 1.0))
            glyph.coordinates.toInt()
        if hasattr(glyph, "program"):
            glyph.program.fromBytecode(b"")
        glyph.recalcBounds(glyf)
        advance, _ = hmtx[name]
        hmtx[name] = (round(advance * WIDTH), getattr(glyph, "xMin", 0))
    space = font.getBestCmap()[ord(" ")]
    hmtx[space] = (round(SPACE_EM * font["head"].unitsPerEm), 0)


def widen_positioning(value) -> None:
    """Every x value of GPOS (kerning, placements, anchors), wherever it nests."""
    if isinstance(value, list):
        for item in value:
            widen_positioning(item)
        return
    if not hasattr(value, "__dict__"):
        return
    for key, item in vars(value).items():
        if key in ("XAdvance", "XPlacement", "XCoordinate") and isinstance(item, int):
            setattr(value, key, round(item * WIDTH))
        else:
            widen_positioning(item)


def drop_hinting(font: TTFont) -> None:
    for tag in ("fpgm", "prep", "cvt "):
        if tag in font:
            del font[tag]
    maxp = font["maxp"]
    for field in ("maxZones", "maxTwilightPoints", "maxStorage", "maxFunctionDefs",
                  "maxInstructionDefs", "maxStackElements", "maxSizeOfInstructions"):
        setattr(maxp, field, 1 if field == "maxZones" else 0)


def rename(font: TTFont) -> None:
    # A modified OFL font keeps its licence and should not pass for the original.
    for record in font["name"].names:
        if record.nameID in (1, 16):
            record.string = FAMILY
        elif record.nameID == 4:
            record.string = FAMILY
        elif record.nameID == 6:
            record.string = FAMILY.replace(" ", "") + "-Regular"
        elif record.nameID == 3:
            record.string = f"{FAMILY.replace(' ', '')}-Regular;{WIDTH}"


def anton_wide() -> TTFont:
    font = TTFont(SOURCES / "Anton-Regular.ttf", recalcTimestamp=False)
    widen_outlines(font)
    if "GPOS" in font:
        widen_positioning(font["GPOS"].table)
    drop_hinting(font)
    font["OS/2"].xAvgCharWidth = round(font["OS/2"].xAvgCharWidth * WIDTH)
    if "DSIG" in font:
        del font["DSIG"]
    rename(font)
    return font


def main() -> None:
    collection = TTCollection()
    collection.fonts = [
        anton_wide(),
        TTFont(SOURCES / "Roboto-Regular.ttf", recalcTimestamp=False),
        TTFont(SOURCES / "Roboto-Bold.ttf", recalcTimestamp=False),
    ]
    # The sources' own timestamps, so a rerun writes the same bytes.
    collection.save(TARGET, shareTables=True)
    print(f"{TARGET.relative_to(ROOT)}: {TARGET.stat().st_size} bytes")


if __name__ == "__main__":
    main()
