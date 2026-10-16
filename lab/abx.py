"""Blind listening kit (brief §1, F7).

    ../.venv/bin/python abx.py            # music from refs/music/*
    ../.venv/bin/python abx.py --synthetic   # self-test with the synthetic mix
    ../.venv/bin/python abx.py --score results.json

Renders reference and Sunbliss with equivalent settings, matches their
integrated loudness to within 0.1 dB (the louder one is turned down, never
up, so nothing new clips), cuts an excerpt around the loudest 20 s and writes
out/abx/: blinded WAV pairs, a static ABX + preference page (index.html)
and key.json (which side is which; the page never reads it).
"""
from __future__ import annotations

import glob
import json
import os
import random
import sys

import numpy as np
import soundfile as sf

import metrics as M
import render as R
from music_synth import mix

OUT = "out/abx"
SCENARIOS = {
    "clipB_+4": dict(clip=True, mode="B", input=4.0),
    "hard_+6": dict(clip=True, mode="Hard", input=6.0),
    "A_+3_sat3A_contour1": dict(clip=True, mode="A", input=3.0, sat=3.0, curve="A", contour=1.0),
    "B_+3_sat4B": dict(clip=True, mode="B", input=3.0, sat=4.0, curve="B"),
}
EXCERPT_S = 20.0


def loudest_excerpt(x, sr, dur):
    n = int(dur * sr)
    if x.shape[1] <= n:
        return 0, x.shape[1]
    hop = sr
    e = [np.mean(x[:, k:k + n] ** 2) for k in range(0, x.shape[1] - n, hop)]
    k = int(np.argmax(e)) * hop
    return k, k + n


def fade(y, sr, ms=20):
    m = int(sr * ms / 1000)
    w = np.linspace(0, 1, m)
    y[:, :m] *= w
    y[:, -m:] *= w[::-1]
    return y


def build(files):
    os.makedirs(OUT, exist_ok=True)
    key, pairs = {}, []
    for path in files:
        if path == "<synthetic>":
            x, sr, name = mix(48000, 30.0), 48000, "synthetic"
        else:
            data, sr = sf.read(path, always_2d=True)
            x, name = data.T.astype(np.float64), os.path.splitext(os.path.basename(path))[0]
            if x.shape[0] == 1:
                x = np.vstack([x, x])
            x = x[:2]
        a0, a1 = loudest_excerpt(x, sr, EXCERPT_S)
        for sc, s in SCENARIOS.items():
            ref = R.ref(x, sr, **s)[:, a0:a1]
            ours = R.ours(x, sr, **s)[:, a0:a1]
            lr, lo = M.lufs(ref, sr), M.lufs(ours, sr)
            # turn the louder one down to match (never up)
            if lo > lr:
                ours *= 10 ** ((lr - lo) / 20)
            else:
                ref *= 10 ** ((lo - lr) / 20)
            lr2, lo2 = M.lufs(ref, sr), M.lufs(ours, sr)
            assert abs(lr2 - lo2) < 0.1, (lr2, lo2)
            pid = f"{name}__{sc}"
            flip = random.random() < 0.5
            a, b = (ours, ref) if flip else (ref, ours)
            for tag, y in (("A", a), ("B", b)):
                sf.write(f"{OUT}/{pid}_{tag}.wav", fade(y.copy(), sr).T, sr, subtype="FLOAT")
            key[pid] = dict(A="sunbliss" if flip else "reference", B="reference" if flip else "sunbliss",
                            lufs=[lr2, lo2], level_diff_db=lo2 - lr2)
            pairs.append(pid)
            print(f"{pid}: matched to {lr2:.2f} LUFS (diff {lo2 - lr2:+.3f} dB)")
    json.dump(key, open(f"{OUT}/key.json", "w"), indent=1)
    page(pairs)


def page(pairs):
    html = """<!doctype html><html lang="pt-BR"><head><meta charset="utf-8"><title>Sunbliss blind test</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{--bg:#1c1c1c;--panel:#2c2c2c;--text:#d6d6d6;--dim:#8a8a8a;--acc:#ff962e}
body{background:var(--bg);color:var(--text);font:14px system-ui,sans-serif;margin:0;padding:16px;max-width:860px}
h1{font-size:18px} .pair{background:var(--panel);border-radius:4px;padding:12px;margin:12px 0}
button{background:#3e3e3e;color:var(--text);border:0;border-radius:3px;padding:8px 14px;margin:3px;cursor:pointer;font-size:14px}
button.on{background:var(--acc);color:#181818} .dim{color:var(--dim)} .ok{color:#7fd67f}
</style></head><body>
<h1>Teste cego — Sunbliss × referência</h1>
<p class="dim">Os níveis de cada par estão igualados em ±0,1 dB (LUFS integrado). Use fones ou monitores, sempre no mesmo volume.
ABX: ouça A, B e X e diga se X é A ou B. Faça 10 tentativas por par (X é sorteado a cada uma). Depois responda qual você prefere.
A troca entre A/B/X mantém a posição de reprodução. No fim, baixe os resultados e rode <code>abx.py --score results.json</code>.</p>
<div id="pairs"></div>
<button id="dl">Baixar resultados (results.json)</button>
<script>
const PAIRS = __PAIRS__; const TRIALS = 10;
const state = {};
const audios = {};
function el(t,a={},...c){const e=document.createElement(t);Object.assign(e,a);c.forEach(x=>e.append(x));return e}
function play(pid, which){
  const s=state[pid]; const src = which==='X' ? s.x : which;
  const a=audios[pid][src]; const other=audios[pid][src==='A'?'B':'A'];
  const t = Math.max(audios[pid].A.currentTime, audios[pid].B.currentTime);
  other.pause(); a.currentTime = s.playing ? t : 0; a.play(); s.playing=true; s.cur=which;
  ['A','B','X'].forEach(k=>s.btn[k].classList.toggle('on',k===which));
}
function stop(pid){const s=state[pid];audios[pid].A.pause();audios[pid].B.pause();s.playing=false;['A','B','X'].forEach(k=>s.btn[k].classList.remove('on'))}
for(const pid of PAIRS){
  audios[pid]={A:new Audio(pid+'_A.wav'),B:new Audio(pid+'_B.wav')};
  audios[pid].A.loop=audios[pid].B.loop=true;
  const s=state[pid]={x:Math.random()<0.5?'A':'B',trials:[],pref:null,playing:false,btn:{}};
  const info=el('span',{className:'dim'}); const box=el('div',{className:'pair'},el('b',{},pid.replace('__',' — ')),el('br'));
  for(const k of ['A','B','X']){s.btn[k]=el('button',{textContent:k,onclick:()=>play(pid,k)});box.append(s.btn[k]);}
  box.append(el('button',{textContent:'■',onclick:()=>stop(pid)}),el('br'));
  const ans=(g)=>{ if(s.trials.length>=TRIALS) return; s.trials.push({guess:g,truth:s.x}); s.x=Math.random()<0.5?'A':'B'; stop(pid);
    info.textContent=` tentativa ${s.trials.length}/${TRIALS}`+(s.trials.length===TRIALS?` — acertos: ${s.trials.filter(t=>t.guess===t.truth).length}/${TRIALS}`:''); };
  box.append(el('button',{textContent:'X é A',onclick:()=>ans('A')}),el('button',{textContent:'X é B',onclick:()=>ans('B')}),info,el('br'));
  const pref=el('span',{className:'dim',textContent:' preferência: —'});
  box.append(el('button',{textContent:'Prefiro A',onclick:()=>{s.pref='A';pref.textContent=' preferência: A'}}),
             el('button',{textContent:'Prefiro B',onclick:()=>{s.pref='B';pref.textContent=' preferência: B'}}),pref);
  document.getElementById('pairs').append(box);
}
document.getElementById('dl').onclick=()=>{
  const out={}; for(const pid of PAIRS) out[pid]={trials:state[pid].trials,preference:state[pid].pref};
  const a=el('a',{href:URL.createObjectURL(new Blob([JSON.stringify(out,null,1)],{type:'application/json'})),download:'results.json'}); a.click();
};
</script></body></html>"""
    open(f"{OUT}/index.html", "w").write(html.replace("__PAIRS__", json.dumps(pairs)))
    print(f"wrote {OUT}/index.html with {len(pairs)} pairs")


def score(path):
    from math import comb
    res = json.load(open(path))
    key = json.load(open(f"{OUT}/key.json"))
    lines = ["| Par | Acertos ABX | p (binomial, 1 cauda) | Preferência |", "|---|---|---|---|"]
    for pid, r in res.items():
        n = len(r["trials"])
        k = sum(t["guess"] == t["truth"] for t in r["trials"])
        p = sum(comb(n, i) for i in range(k, n + 1)) / 2 ** n if n else float("nan")
        pref = key[pid][r["preference"]] if r.get("preference") else "—"
        lines.append(f"| {pid} | {k}/{n} | {p:.3f} | {pref} |")
    print("\n".join(lines))
    return "\n".join(lines)


if __name__ == "__main__":
    if "--score" in sys.argv:
        score(sys.argv[sys.argv.index("--score") + 1])
    else:
        files = ["<synthetic>"] if "--synthetic" in sys.argv else sorted(
            f for e in ("wav", "flac", "aif", "aiff") for f in glob.glob(f"../refs/music/*.{e}"))
        if not files:
            sys.exit("no files in refs/music/ (use --synthetic for a self-test)")
        random.seed()
        build(files)
