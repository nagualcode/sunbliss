# Cadeia de processamento

## Referência (inferida e confirmada)

O manual da referência (p. 23) traz este fluxograma:

```
Input → Box Tone → Input Trim → Upsample → Clipper → Alchemy → Gold → Downsample → Output Trim → Dry/Wet → Output
  └→ Bypass / Delay Compensation ───────────────────────────────────────────────────────────────┘
```

Confirmações por caixa-preta:

| Afirmação | Teste | Resultado |
|---|---|---|
| Tone antes do clipper | clip Hard a +6 dB, seno de 7 kHz a 44,1 kHz; H3 (21 kHz) com Tone Flat × Classic | 58,17 × 58,09 dB: o Tone (−7,8 dB a 21 kHz) **não** filtrou os harmônicos ⇒ fica antes |
| Gold depois do clipper | curva Gold normalizada pelo ceiling | idêntica a ceiling 0 e −6 (8e−5) ⇒ opera no domínio já limitado a *c* |
| Alchemy relativo ao ceiling | 8 kHz a −0,5 dB rel. ceiling −6 | −1,86 dB, igual a ceiling 0 |
| Output depois do mixer | dry-only + Output +6 | +6,0 dB |
| Dry = entrada sem processamento | wet 0 / dry 100 | null |
| Delta = (wet − entrada) × output | clip Hard +6 dB | null −139 dB |
| Clip Filter antes do clipper | harmônicos do clip Hard com/sem filtro | Δ ≤ 0,01 dB |

A hipótese inicial do briefing (saturação → contour → clipper) estava **errada**. Na
referência a ordem é clipper → contour → saturação. Isso faz sentido: a
saturação é uma curva que leva [0, c] em [0, c], e só funciona se a entrada já
estiver limitada a c.

## Sunbliss

```
x ─┬─ Tone (base rate, FIR fase linear) ─ ×g_in ─ ↑L ─┬─ [Clip Filter pré] ─ Clipper(mode, ADAA2) ─ Contour ─ Saturation ─ ↓L (decimação "conversor") ─ limitador (pico amostral | true-peak) ─ ×unity ─┐
   │                                                   └─ (clip off: passa direto) ──────────────┘                                                                  │
   └─ atraso = latência total ─────────────────────────────────────────────── dry ──────────────────────────────────── mix (wet, dry, mutes, delta) ─ ×g_out ─ bypass xfade ─ y
```

- Todo o processamento não linear roda na taxa com oversampling L.
- O limitador de saída (pico amostral com o guard desligado, true-peak com ele ligado) tem lookahead de ~1,8 ms, sempre presente, para a latência não mudar ao ligar o guard. O dry acompanha.
- O Contour não tem detector separado nem lookahead: o ganho da banda alta vem do nível |x|/c do próprio sinal clipado, na taxa de oversampling, suavizado como média cúbica (τ 0,4 ms), como na referência (ver `docs/CONTROLS.md`).
- O Unity vem depois do guard, então reduz o pico de verdade (comportamento da referência: "reduz a saída após todo o processamento").
