"""The fast preview's oracle: the PDF the plugin shows against a clean full build of the same sources.

Usage: python check.py SHOWN.pdf ORACLE.pdf MANIFEST.json OUT.json

MANIFEST.json: {"stale": [[from, to, why], ...]} (result pages from 0, end excluded) and "landed": true when SHOWN is
the background full build's PDF (then nothing may differ). Writes OUT.json: {check: {"pass": bool, "detail": str}}.

A page that is not marked stale claims to be current: its text, citation numbers and links must be the oracle's. A
stale page may differ (the plugin says so on the page and in the status bar) until the full build lands.

It reads the PDFs with PyMuPDF (https://pymupdf.readthedocs.io), an implementation independent of the plugin's own
PDF code, so that an error in one is not hidden by the same error in the other."""
import json
import re
import sys

import pymupdf

STALE_MARK = re.compile(r"Preview: .* until the full build")


def norm(t):
    t = STALE_MARK.sub("", t)
    return re.sub(r"\s+", " ", t).strip()


def pages_text(doc):
    return [norm(p.get_text()) for p in doc]


def cite_numbers(text):
    """The numbers in numeric citations, in order: [3], [3, 5], [3-5], [3, p. 27]."""
    out = []
    for m in re.finditer(r"\[(\d+(?:\s*[,–-]\s*\d+)*)(?:,\s*[^\]]*)?\]", text):
        out.append(m.group(1).replace(" ", ""))
    return out


def dest_map(doc):
    """Each named destination: (page, y rounded)."""
    out = {}
    for name, d in doc.resolve_names().items():
        page = d.get("page", -1)
        to = d.get("to") or (0, 0)
        out[name] = (page, round(to[1]))
    return out


def links(doc):
    out = []
    for k, p in enumerate(doc):
        for l in p.get_links():
            if l.get("kind") == pymupdf.LINK_NAMED and l.get("nameddest"):
                out.append((k, l["nameddest"], tuple(round(x) for x in l["from"])))
            elif l.get("kind") == pymupdf.LINK_GOTO:
                out.append((k, "#page%d" % l.get("page", -1), tuple(round(x) for x in l["from"])))
    return out


def section(texts, start_pat, end_pat=None, last=False):
    """The text from start_pat (its first occurrence, or its last: a heading the contents also list) to the end, or
    to end_pat."""
    joined = " \f ".join(texts)
    i = joined.rfind(start_pat) if last else joined.find(start_pat)
    if i < 0:
        return ""
    j = joined.find(end_pat, i + len(start_pat)) if end_pat else -1
    return joined[i:] if j < 0 else joined[i:j]


def main():
    shown_path, oracle_path, manifest_path, out_path = sys.argv[1:5]
    shown, oracle = pymupdf.open(shown_path), pymupdf.open(oracle_path)
    manifest = json.load(open(manifest_path, encoding="utf-8"))
    landed = manifest.get("landed", False)
    stale = set()
    for a, b, _why in ([] if landed else manifest.get("stale", [])):
        stale.update(range(a, b))
    st, ot = pages_text(shown), pages_text(oracle)
    res = {}

    def put(name, ok, detail=""):
        res[name] = {"pass": bool(ok), "detail": detail}

    put("page count", len(st) == len(ot) or bool(stale), f"shown {len(st)}, full build {len(ot)}")
    current = [k for k in range(len(st)) if k not in stale]
    bad = [k for k in current if k >= len(ot) or st[k] != ot[k]]
    detail = ""
    if bad:
        k = bad[0]
        a, b = st[k], ot[k] if k < len(ot) else ""
        i = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), min(len(a), len(b)))
        detail = f"page {k + 1}: shown ...{a[max(0, i - 40):i + 60]!r} / full build ...{b[max(0, i - 40):i + 60]!r}"
    put("text of current pages", not bad, f"{len(current)} current, {len(stale)} stale; differ: {[k + 1 for k in bad]} {detail}")
    # citation numbers on current pages
    cbad = [k + 1 for k in current if k < len(ot) and cite_numbers(st[k]) != cite_numbers(ot[k])]
    put("citation numbers on current pages", not cbad, f"differ on pages {cbad}")
    # the bibliography, if its pages are current
    sb, ob = section(st, "References"), section(ot, "References")
    bib_pages = [k for k in range(len(st)) if "References" in st[k]]
    bib_current = all(k not in stale for k in range(bib_pages[-1] if bib_pages else len(st), len(st)))
    put("bibliography order and entries", (sb == ob) or not bib_current, "current" if bib_current else "stale (waits for the full build)")
    # the generated lists
    for title, nxt in [("Contents", "List of Figures"), ("List of Figures", "List of Tables"), ("List of Tables", "Introduction")]:
        a, b = section(st, title, nxt), section(ot, title, nxt)
        pages = [k for k in range(len(st)) if title in st[k]]
        cur = all(k not in stale for k in pages)
        put(f"{title} entries", a == b or not cur, "current" if cur else "stale")
    a, b = section(st, "Terms and Definitions", "References", True), section(ot, "Terms and Definitions", "References", True)
    pages = [k for k in range(len(st)) if "Terms and Definitions" in st[k] and "Every term" in st[k]]
    cur = all(k not in stale for k in pages)
    put("Terms list entries and page lists", a == b or not cur, "current" if cur else "stale")
    # links: every named link must resolve, to the oracle's page and place for that name
    sd, od = dest_map(shown), dest_map(oracle)
    dangling, wrong = [], []
    for k, name, rect in links(shown):
        if name.startswith("#page"):
            continue
        if name not in sd:
            dangling.append((k + 1, name))
            continue
        if name in od and (k not in stale) and (sd[name][0] not in stale) and (sd[name][0] != od[name][0] or abs(sd[name][1] - od[name][1]) > 2):
            wrong.append((k + 1, name, sd[name], od[name]))
    put("links resolve", not dangling, f"dangling: {dangling[:8]}")
    put("links point where the full build's do", not wrong, f"{len(wrong)} wrong: {wrong[:6]}")
    # the name tree: the names of the oracle all there (on current pages), and every name's page a page of the PDF
    missing = [n for n, (p, _) in od.items() if n not in sd and p not in stale]
    out_of_range = [n for n, (p, _) in sd.items() if p < 0 or p >= len(st)]
    put("name tree consistent", not missing and not out_of_range, f"missing {missing[:8]}, bad pages {out_of_range[:8]}")
    # no text twice: no line of the shown PDF (of 25 characters or more) occurs twice or more unless the full build has it as
    # often (a stale page may differ from the full build, but never repeat what another page shows)
    from collections import Counter
    def lines(doc):
        return Counter(l for p in doc for l in (norm(x) for x in p.get_text().splitlines()) if len(l) >= 25 and not STALE_MARK.search(l))
    ls, lo = lines(shown), lines(oracle)
    extra = {l: n for l, n in ls.items() if n >= 2 and n > lo.get(l, 0)}
    put("no text twice", not extra, f"{sum(extra.values())} lines more often than in the full build: {list(extra)[:3]}")
    json.dump(res, open(out_path, "w", encoding="utf-8"), indent=1)


main()
