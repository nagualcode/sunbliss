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
            L.append(f"| Curva estática DC, clip {m} (amplitude ≥ −6 dB) | ≤ 0,5 dB | máx {v['max_db']:.3f} dB (média {v['mean_db']:.3f}) | {'✅' if v['max_db'] <= 0.5 else '❌'} |")
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
    L = ["\n### Superioridade (§7.2) — mesma grade, 48 kHz, High\n",
         "| Métrica | Referência | Sunbliss | Vence? |", "|---|---|---|---|"]

    def row(name, ref, ours, win, fmt="{:.1f}"):
        L.append(f"| {name} | {fmt.format(ref)} | {fmt.format(ours)} | {'✅' if win else '❌'} |")

    if "aliasing" in s:
        for k, (r, o) in s["aliasing"].items():
            mode, f = k.split("_")
            row(f"Aliasing+ruído, tom {int(f)//1000} kHz +6 dB, clip {mode} (dB rel. fund.)", r, o, o < r)
    if "imd" in s:
        for k, (r, o) in s["imd"].items():
            row(f"Produtos < 18 kHz, 19+20 kHz +3 dB, clip {k.split('_')[0]} (dB)", r, o, o < r)
    if "true_peak" in s:
        for k, v in s["true_peak"].items():
            row(f"True-peak na mixagem, {k} (dBTP; nosso com guard)", v["tp_ref"], v["tp_ours"], v["tp_ours"] <= 0 < v["tp_ref"] or v["tp_ours"] < v["tp_ref"], "{:+.2f}")
            row(f"  loudness no mesmo ajuste (LUFS)", v["lufs_ref"], v["lufs_ours"], v["lufs_ours"] >= v["lufs_ref"] - 0.3, "{:.2f}")
    if "transparency" in s:
        t = s["transparency"]
        for m in ["A", "Hard", "B"]:
            row(f"Resíduo abaixo do teto (−6 dB), clip {m}, banda 20 kHz (dB rms)", t[m][0], t[m][1], t[m][1] <= t[m][0] + 0.2)
        row("Tudo neutro: resíduo de pico (dBFS)", t["neutral"][0], t["neutral"][1], t["neutral"][1] < t["neutral"][0])
    if "recovery" in s:
        for m, (r, o) in s["recovery"].items():
            row(f"Recuperação pós-rajada +12 dB, clip {m}: erro do sinal de prova (dB)", r, o, o < r, "{:.3f}")
    if "automation" in s:
        for k, (r, o) in s["automation"].items():
            row(f"Clique na troca '{k}': excesso de HF > 16 kHz em ±5 ms (dB)", r, o, o <= max(r, 0.5), "{:+.2f}")
    if "cpu" in s:
        c = s["cpu"]
        row("CPU, mesmo host (pedalboard), High, estéreo 48 kHz (% de tempo real)", 100 * c["reference_high"], 100 * c["ours_high"], c["ours_high"] < c["reference_high"], "{:.2f}")
        L.append(f"| CPU Pristine / Extra (nosso; a referência não expõe a troca) | — | {100*c['ours_pristine']:.2f} / {100*c['ours_extra']:.2f} | — |")
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
