"""The chapters latex_sync.py gives for a folder of notes, for python.rs's real_notes test:

    python -I python_reference.py NOTES_FOLDER OUT_FOLDER
    LATEX_NOTES=NOTES_FOLDER LATEX_EXPECTED=OUT_FOLDER cargo test -p latex-core --test python

Reads the notes (the ones latex_sync.NOTES lists) and writes <note>.tex and summary.json (the warnings) into OUT_FOLDER
only; nothing is written into the notes folder or its compiled folder.

The script was archived on 2026-10-25, when this plugin replaced it: it is imported from the archived copy,
archived/latex-sync-python/latex_sync.py (formerly zotsearch/tools/latex_sync.py)."""
import json, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / "archived" / "latex-sync-python"))
import latex_sync as ls

NOTES, OUT = Path(sys.argv[1]), Path(sys.argv[2])
OUT.mkdir(parents=True, exist_ok=True)
ls.NOTES_DIR = NOTES  # commands() reads the preamble note from here (read only)
for name in ("TEX_DIR", "STATE_FILE", "BACKUPS", "SYNC_LOG"):  # nothing below writes; never the real compiled folder all the same
    setattr(ls, name, OUT / f"unused-{name.lower()}")
notes = ls.read_notes()
ls.collect_headings(notes)
summary = {}
for note, md in notes.items():
    try:
        tex, warn = ls.md_to_tex(note, md)
    except Exception as e:  # noqa: BLE001 (any failure is the test's answer)
        summary[note] = {"error": f"{type(e).__name__}: {e}"}
        continue
    (OUT / (note[:-3] + ".tex")).write_text(tex, encoding="utf-8", newline="\n")
    summary[note] = {"tex": ls.NOTES[note], "warnings": warn}
(OUT / "summary.json").write_text(json.dumps(summary, indent=1, ensure_ascii=False), encoding="utf-8")
print(json.dumps({k: (v.get("error") or len(v["warnings"])) for k, v in summary.items()}))
