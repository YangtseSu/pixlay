#!/usr/bin/env python3
"""Regenerates the pinned test font.

`pixlay-render`'s text tests measure line breaking and glyph advances, which no
two system fonts agree on: without a pinned font the tests would be a statement
about this machine's font set, and in a clean chroot (`S8`) there would be no
font at all. So the tests pin one, and this script produces it.

    python3 crates/pixlay-cli/tests/fixtures/fonts/generate.py

It lives with `pixlay-cli`'s other fixtures because two crates use it: the text
measurements in `pixlay-render` and the end-to-end render test in `pixlay-cli`.

The source is Arch's `noto-fonts-cjk` (`NotoSansCJK-Regular.ttc`, SIL Open Font
License 1.1, `OFL.txt` in this directory), subset to the characters the tests
and the text fixture use. Two reasons it is a *subset*:

* size — the full collection is 19 MB and this is a few tens of KB, and the
  repository is packed into an AUR source tarball;
* determinism — file name, family name and glyph set are all fixed here, so a
  regeneration is the same file.

The OFL forbids releasing a modified version under the Reserved Font Name
"Source" (Noto Sans CJK derives from Source Han Sans), so the subset is renamed
to "Pixlay Test Sans" — the family name is what `fonts.conf` (written by
`tests/text/fonts.rs` at runtime) maps `sans-serif` to.
"""

import subprocess
import sys
import tempfile
from pathlib import Path

from fontTools.ttLib import TTFont

HERE = Path(__file__).parent
SOURCE = Path("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc")
FACE = 2  # "Noto Sans CJK SC"
OUTPUT = HERE / "pixlay-test-sans.otf"
FAMILY = "Pixlay Test Sans"

# Every character the text tests and `crates/pixlay-cli/tests/fixtures/text.pixlay`
# draw. Adding a character to a test means adding it here and regenerating;
# `tests/text/coverage.rs` fails when the two have drifted apart.
ASCII = "".join(chr(code) for code in range(0x20, 0x7F))
CHARS = (
    ASCII
    + "\u2588"  # FULL BLOCK: the geometric probe glyph
    + "\u3000"  # ideographic space
    + "他说今天气很好然后走了这是一段用来测试断行的中文文字标点不应该出现在行首连续"
    + "的压缩括号里的内容后面继续写下去看看价格元质量值得购买日本语汉字符号拍摄于"
    + "“”‘’（）「」『』【】〈〉《》〔〕｛｝［］〘〙〚〛、。，．：；？！—…·ー〜‥・"
)


def main() -> int:
    if not SOURCE.is_file():
        print(f"missing source font: {SOURCE}", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory() as scratch:
        text_file = Path(scratch) / "chars.txt"
        text_file.write_text("".join(sorted(set(CHARS))), encoding="utf-8")
        subprocess.run(
            [
                "pyftsubset",
                str(SOURCE),
                f"--font-number={FACE}",
                f"--text-file={text_file}",
                f"--output-file={OUTPUT}",
                # `*` rather than a list: the punctuation compression the
                # renderer asks for is the OpenType `halt` feature, and a
                # subset that drops it would silently stop compressing.
                "--layout-features=*",
                "--glyph-names",
                "--notdef-outline",
                "--name-IDs=*",
                "--drop-tables+=DSIG",
                "--recalc-bounds",
            ],
            check=True,
        )

    font = TTFont(OUTPUT)
    rename(font)
    font.save(OUTPUT)
    report(OUTPUT, font)
    return 0


def rename(font: TTFont) -> None:
    """Give the subset its own name, as the OFL's Reserved Font Name requires."""
    full = f"{FAMILY} Regular"
    postscript = FAMILY.replace(" ", "") + "-Regular"
    values = {
        0: "Subset of Noto Sans CJK SC (SIL Open Font License 1.1); renamed. See OFL.txt.",
        1: FAMILY,
        2: "Regular",
        3: f"{FAMILY}:subset",
        4: full,
        5: "Version 1.000",
        6: postscript,
        16: FAMILY,
        17: "Regular",
    }
    name = font["name"]
    for record in list(name.names):
        if record.nameID in values:
            name.setName(values[record.nameID], record.nameID, record.platformID, record.platEncID, record.langID)


def report(path: Path, font: TTFont) -> None:
    codepoints = set(font.getBestCmap())
    missing = sorted(set(map(ord, CHARS)) - codepoints)
    gsub = sorted({record.FeatureTag for record in font["GSUB"].table.FeatureList.FeatureRecord})
    # `halt` is a *positioning* feature (an advance adjustment), so it lives in
    # GPOS — checking GSUB alone would report it missing.
    gpos = sorted({record.FeatureTag for record in font["GPOS"].table.FeatureList.FeatureRecord})
    features = gsub + gpos
    print(f"{path.name}: {path.stat().st_size} bytes, {len(font.getGlyphOrder())} glyphs")
    print(f"  covered: {len(codepoints)} codepoints; missing: {[hex(c) for c in missing]}")
    print(f"  GSUB features: {' '.join(gsub)}")
    print(f"  GPOS features: {' '.join(gpos)}")
    if missing:
        raise SystemExit("the subset does not cover every character the tests use")
    if "halt" not in features:
        raise SystemExit("the subset lost the `halt` feature (punctuation compression)")


if __name__ == "__main__":
    raise SystemExit(main())
