"""The four sizes any text on a slide is allowed to be, and the words budget.

These are floors, not targets. A slide whose content will not fit at these sizes
is a slide that gets split, never a slide whose type shrinks — `MINIMUM_SIZE` is
asserted over every generated text element in test_scenes.py, so shrinking one
label to make a layout work fails the build instead of quietly shipping.

`WORDS_PER_SLIDE` is the default budget for everything on a slide except its
title. Slides that hold an enumeration rather than prose exceed it by design and
are named, with a reason, in `WORD_BUDGET_CEILINGS` in test_scenes.py.
"""

from __future__ import annotations

TITLE_SIZE = 46.0
HEADING_SIZE = 30.0
BODY_SIZE = 24.0
CAPTION_SIZE = 18.0

MINIMUM_SIZE = CAPTION_SIZE

WORDS_PER_SLIDE = 35
