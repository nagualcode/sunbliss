# Sunbliss — relatório do projeto

Plugin de clipper/saturação de masterização em Rust (truce), com paridade de
controles com a referência (Gold Clip 1.5.0) e modelagem própria do overload
de conversor e da Soft Saturation do manual da Lavry AD122-96 MkIII.

Os números deste relatório vêm de `lab/` (scripts reprodutíveis) e de
`docs/data/*.json`. Tudo o que fica entre os marcadores AUTO é gerado por
`lab/report.py`.

## Estado por fase

| Fase | Estado | Notas |
|---|---|---|
| F0 Ambiente | ✅ | `docs/ENV.md`. A referência carrega headless (só AU). Latência e aquecimento compensados no lab. |
| F1 Inventário | ✅ | 23 parâmetros mapeados (`docs/goldclip_params.json`, `docs/CONTROLS.md`). Cadeia confirmada por caixa-preta (`docs/CHAIN.md`): a ordem real é clipper → contour → saturação. |
| F2 Laboratório | ✅ | `lab/goldclip.py`, `stimuli.py`, `metrics.py`, `render.py`, `parity.py`, `report.py`. Baseline da referência em `docs/data/gc_*.json` e figuras em `docs/fig/`. |
| F3 Núcleo DSP | ✅ | `crates/core` (f64, sem alocação, testado) + `crates/cli`. Paridade abaixo. |
| F4 Blocos restantes | ✅ | Contour, Tone (`docs/TONE.md`), mixer/delta, unity, guard de true-peak. A/B fica no plugin (F5). |
| F5 Plugin truce | em andamento | — |
| F6 Superioridade | em andamento | primeiros números abaixo |
| F7 Teste cego | pendente | precisa de música real em `refs/music/` |

## Decisões de modelagem (resumo)

- **Curva B (Soft Saturation do manual):** ganho g abaixo de T = −2G dBFS e exponencial C1 acima, com fechamento **(b)** y(FS) = FS. O gráfico do Apêndice III, digitalizado, é explicado com **rms 0,031 dB (+3) e 0,033 dB (+6)**. O fechamento (a) dá 0,040/0,072 dB e prevê −0,22 dB em 0 dBFS, o que contradiz o gráfico e a lâmpada MAX. Dados: `docs/data/lavry_fit.json`, `lavry_manual_curve_digitized.json`.
- **Curva A:** linear até 0,5 c e quadrática C1 até (c, c). Inclinação final 4 − 3g, e g ≤ 4/3 (+2,5 dB) por construção. Erro ≤ 0,06 dB contra a referência.
- **Knob contínuo (decisão de design):** na curva B o amount é G em dB, com T = −2G (generalização do padrão do manual). Na curva A, mapa linear por partes 0 → 0, 2 → 1,32, 6 → 2,50 dB, medido na referência.
- **Clipper:** curvas estáticas medidas por degraus de DC, com **ADAA de 1ª ordem** na taxa de oversampling. O meio-sample de atraso do ADAA é compensado por um interpolador de comprimento par. O roll-off do ADAA é compensado no decimador.
- **Converter B:** curva assimétrica medida (H2 dominante desde −30 dBFS, como na referência) e "memória de sobrecarga" (τ 72 ms acima de 1,33 c). **Não** reproduzimos a divergência da referência em drive extremo (inversão de polaridade / travamento): é um defeito.
- **Decimador de conversor:** fase linear, plano até 0,4535·fs, **stopband a partir de fs/2** (140 dB), sem aliasing na banda. A referência usa meia-banda e aceita aliasing entre 20 e 24 kHz.
- **Overshoot Guard:** limitador de true-peak com estimador 8× + refinamento parabólico, mínimo móvel com hold e média (sem cliques), linkado entre canais. Com o guard ligado, o clamp amostral é desligado, porque o clamp recria energia em Nyquist e com ela os ISPs.
- **Anti-overshoot da referência:** ela faz "ducking" adaptativo (até −2 dB, hold ~0,4 s) depois de bordas íngremes acima do teto. Não replicamos: é bombeamento. Por isso, em drive alto, o Sunbliss sai mais alto que a referência (ver loudness).

<!-- AUTO:BEGIN -->
### Paridade (§7.1) — Sunbliss × referência, 48 kHz, qualidade High

| Teste | Critério | Resultado | OK |
|---|---|---|---|
| Curva estática DC, clip A (|x| ≥ −6 dB) | ≤ 0,5 dB | máx 0.022 dB (média 0.003) | ✅ |
| Curva estática DC, clip B (|x| ≥ −6 dB) | ≤ 0,5 dB | máx 0.111 dB (média 0.012) | ✅ |
| Curva estática DC, clip Hard (|x| ≥ −6 dB) | ≤ 0,5 dB | máx 0.185 dB (média 0.058) | ✅ |
| Seno 1 kHz −12…+8 dB, clip A: RMS de saída | ≤ 0,5 dB | máx 0.016 dB | ✅ |
| Seno 1 kHz −12…+8 dB, clip B: RMS de saída | ≤ 0,5 dB | máx 0.106 dB | ✅ |
| Seno 1 kHz −12…+8 dB, clip Hard: RMS de saída | ≤ 0,5 dB | máx 0.002 dB | ✅ |
| Curva de saturação A, amounts 1…6 (x > −30 dB) | ≤ 0,5 dB | máx 0.143 dB | ✅ |
| Curva de saturação B, amounts 1…6 (x > −30 dB) | ≤ 0,5 dB | máx 0.206 dB | ✅ |
| Unity, curva A, amount 6 (nível baixo, ganho líquido) | mesmo efeito | ref +0.030 dB, nosso +0.000 dB | ✅ |
| Unity, curva B, amount 6 (nível baixo, ganho líquido) | mesmo efeito | ref -0.004 dB, nosso +0.000 dB | ✅ |
| Contour 2 dB: estático 8 kHz e curva de frequência | ≤ 0,5 dB | máx 0.156 dB | ✅ |
| Resposta linear filter_A (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.035 dB | ✅ |
| Resposta linear filter_B (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.077 dB | ✅ |
| Resposta linear filter_Hard (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.035 dB | ✅ |
| Resposta linear tone_A (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.006 dB | ✅ |
| Resposta linear tone_B (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.004 dB | ✅ |

#### Loudness na mixagem sintética (12 s)

| Ajuste | Ref LUFS | Nosso LUFS | Δ LU | Δ LU (guard on) | TP ref | TP nosso | TP nosso+guard |
|---|---|---|---|---|---|---|---|
| clipB_+6 | -13.09 | -13.01 | +0.08 | +0.02 | +1.14 | +0.40 | -0.15 |
| clipA_+6 | -13.01 | -12.86 | +0.15 | +0.06 | +0.26 | +0.58 | -0.15 |
| hard_+6 | -13.14 | -12.86 | +0.29 | +0.17 | +0.34 | +0.68 | -0.15 |
| hard_+10 | -9.99 | -9.24 | +0.75 | +0.50 | +0.62 | +1.21 | -0.12 |
| B_+6_satB3 | -10.76 | -10.33 | +0.44 | +0.29 | +0.56 | +0.95 | -0.14 |
| A_+6_satA6 | -11.00 | -10.61 | +0.39 | +0.22 | +0.35 | +1.00 | -0.14 |
| hard_+6_contour2 | -13.07 | -13.11 | -0.05 | -0.06 | +0.23 | +0.01 | -0.15 |
| B_+8_all | -8.75 | -8.27 | +0.49 | +0.37 | +1.15 | +0.86 | -0.14 |

<!-- AUTO:END -->

## O que falta

- F5: plugin truce (parâmetros, GUI original, presets, A/B, Clip Meter, validação em DAWs).
- F6: varredura completa de aliasing (5/9/13/15 kHz), IMD, CPU por modo, recuperação pós-clip.
- F7: pares nivelados e ABX. Precisa de música real em `refs/music/` (não baixamos música).
