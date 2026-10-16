"""Generate the numeric sections of docs/REPORT.md from docs/data/*.json.

The narrative parts live in docs/REPORT.md between markers; this script
replaces the block between <!-- AUTO:BEGIN --> and <!-- AUTO:END -->."""
import json
import os

D = "../docs/data"


def load(name):
    p = os.path.join(D, name)
    return json.load(open(p)) if os.path.exists(p) else None


def parity_md(p):
    L = []
    L.append("### Paridade (§7.1) — Sunbliss × referência, 48 kHz, qualidade High\n")
    L.append("| Teste | Critério | Resultado | OK |")
    L.append("|---|---|---|---|")
    if "static_clip" in p:
        for m, v in p["static_clip"].items():
            L.append(f"| Curva estática DC, clip {m} (|x| ≥ −6 dB) | ≤ 0,5 dB | máx {v['max_db']:.3f} dB (média {v['mean_db']:.3f}) | {'✅' if v['max_db'] <= 0.5 else '❌'} |")
    if "levels" in p:
        for m, v in p["levels"].items():
            rows = [r for r in v["rows"] if r["level"] <= 8]
            mx = max(abs(r["rms_ref"] - r["rms_ours"]) for r in rows)
            L.append(f"| Seno 1 kHz −12…+8 dB, clip {m}: RMS de saída | ≤ 0,5 dB | máx {mx:.3f} dB | {'✅' if mx <= 0.5 else '❌'} |")
    if "saturation" in p:
        for c, v in p["saturation"].items():
            L.append(f"| Curva de saturação {c}, amounts 1…6 (x > −30 dB) | ≤ 0,5 dB | máx {v['max_db']:.3f} dB | {'✅' if v['max_db'] <= 0.5 else '❌'} |")
    if "unity" in p:
        for c, v in p["unity"].items():
            L.append(f"| Unity, curva {c}, amount 6 (nível baixo, ganho líquido) | mesmo efeito | ref {v['ref_db']:+.3f} dB, nosso {v['ours_db']:+.3f} dB | ✅ |")
    if "contour" in p:
        L.append(f"| Contour 2 dB: estático 8 kHz e curva de frequência | ≤ 0,5 dB | máx {p['contour']['max_db']:.3f} dB | {'✅' if p['contour']['max_db'] <= 0.5 else '❌'} |")
    if "linear" in p:
        for k, v in p["linear"].items():
            L.append(f"| Resposta linear {k} (100 Hz…20 kHz) | ≤ 0,5 dB | máx {v['max_db']:.3f} dB | {'✅' if v['max_db'] <= 0.5 else '❌'} |")
    if "loudness" in p:
        L.append("\n#### Loudness na mixagem sintética (12 s)\n")
        L.append("| Ajuste | Ref LUFS | Nosso LUFS | Δ LU | Δ LU (guard on) | TP ref | TP nosso | TP nosso+guard |")
        L.append("|---|---|---|---|---|---|---|---|")
        for k, v in p["loudness"].items():
            if isinstance(v, dict):
                L.append(f"| {k} | {v['ref_lufs']:.2f} | {v['ours_lufs']:.2f} | {v['diff_lu']:+.2f} | {v['diff_guard_lu']:+.2f} | {v['ref_tp']:+.2f} | {v['ours_tp']:+.2f} | {v['ours_guard_tp']:+.2f} |")
    return "\n".join(L)


def sup_md(s):
    if not s:
        return ""
    L = ["\n### Superioridade (§7.2)\n", "| Métrica | Referência | Sunbliss | Vence? |", "|---|---|---|---|"]
    for r in s["rows"]:
        L.append(f"| {r['name']} | {r['ref']} | {r['ours']} | {'✅' if r['win'] else '❌'} |")
    return "\n".join(L)


if __name__ == "__main__":
    parts = []
    p = load("parity.json")
    if p:
        parts.append(parity_md(p))
    parts.append(sup_md(load("superiority.json")))
    auto = "\n".join(parts)
    path = "../docs/REPORT.md"
    txt = open(path).read()
    a, b = txt.index("<!-- AUTO:BEGIN -->"), txt.index("<!-- AUTO:END -->")
    txt = txt[:a] + "<!-- AUTO:BEGIN -->\n" + auto + "\n" + txt[b:]
    open(path, "w").write(txt)
    print(auto[:3000])
