# Sunbliss — relatório do projeto

Plugin de clipper/saturação de masterização em Rust (truce), com paridade de
controles com a referência (Gold Clip 1.5.0) e modelagem própria do overload
de conversor e da Soft Saturation do manual da Lavry AD122-96 MkIII / MX.

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
| F5 Plugin truce | ✅ (exceto DAWs) | CLAP/VST3/AU instalados; **auval: AU VALIDATION SUCCEEDED**; VST3 e AU produzem o mesmo áudio que o CLI (pior caso −140 dBFS, limite do f32); GUI original (`docs/fig/gui.png`); 6 presets de fábrica; A/B salvo na sessão; Clip Meter com posição. Teste em Ableton/Reaper/Logic depende do usuário. |
| F6 Superioridade | ✅ com ressalvas | ver tabela: vence em aliasing (11/12; perde 0,8 dB no Hard 15 kHz), true-peak, transparência, cliques e CPU (68 contra 113 Mciclos por s de áudio, medidos pelo contador de ciclos do processo); perde 1 dB no IMD do Hard. Com buffer de 512 a "recuperação pós-rajada" deixou de ser vitória: a vantagem medida antes era o ducking da referência esticado pelo buffer de 8192. |
| F8 White-box | ✅ | Leitura passiva do binário (sem decifrar a proteção) usada só como fonte de hipóteses. Cada hipótese foi confirmada por caixa-preta: ducking por buffer (lab com buffer de 512), Classic = polinômio de grau 9 (`lab/fit_classic.py`), Alchemy = crossover LR 2965 Hz + lei tabelada + média cúbica com τ 0,4 ms (`lab/probe_alchemy_inst.py`, `lab/probe_alchemy_mod.py`). Ver README, seção "Leitura white-box". |
| F7 Teste cego | kit pronto | `lab/abx.py`: pares nivelados (±0,1 dB LUFS), página ABX + preferência (`lab/out/abx/index.html`), placar com p binomial. Autoteste com a mixagem sintética OK. **Falta o usuário ouvir com música real.** |

## Decisões de modelagem (resumo)

- **Curva B (Soft Saturation do manual):** ganho g abaixo de T = −2G dBFS e exponencial C1 acima, com fechamento **(b)** y(FS) = FS. O gráfico do Apêndice III, digitalizado, é explicado com **rms 0,031 dB (+3) e 0,033 dB (+6)**. O fechamento (a) dá 0,040/0,072 dB e prevê −0,22 dB em 0 dBFS, o que contradiz o gráfico e a lâmpada MAX. Dados: `docs/data/lavry_fit.json`, `lavry_manual_curve_digitized.json`.
- **Curva A:** linear até 0,5 c e quadrática C1 até (c, c). Inclinação final 4 − 3g, e g ≤ 4/3 (+2,5 dB) por construção. Erro ≤ 0,06 dB contra a referência.
- **Knob contínuo (decisão de design):** na curva B o amount é G em dB, com T = −2G (generalização do padrão do manual). Na curva A, mapa linear por partes 0 → 0, 2 → 1,32, 6 → 2,50 dB, medido na referência.
- **Clipper:** curvas estáticas medidas por degraus de DC, com **ADAA de 2ª ordem** na taxa de oversampling (segunda antiderivada tabelada, 1 avaliação de F2 por amostra com cache, atalho exato na região linear). Na região linear o ADAA2 é a média de 3 taps, com atraso inteiro de 1 amostra; o roll-off (1+2cos ω)/3 é compensado dentro do decimador. Na simulação, a 4× o ADAA2 dá −82 dB contra −64 dB do ADAA1 num tom de 9 kHz clipado.
- **Converter B:** polinômio de grau 9 sem termo constante, ajustado a 262 platôs DC com bordas suaves em |u| ≤ 1,31 c (resíduo 1,4e−7, o piso do f32 da E/S). A curva tem H2 dominante desde −30 dBFS, como na referência. A "memória de sobrecarga" começa em 1,32 c, a borda do polinômio, com τ = 72,5 ms, ambos medidos. **Não** reproduzimos a inversão de polaridade da referência em drive extremo (≳ 2,5 c).
- **Decimador de conversor:** fase linear, plano até max(20 kHz; 0,4167·fs) (≤ 0,4535·fs), **stopband a partir de fs/2** (100 dB; era 140, sem diferença medida), sem aliasing na banda. A referência usa meia-banda e aceita aliasing entre 20 e 24 kHz. O projeto integra a resposta só até o corte (Simpson), o que tira o ripple de quadratura (~3e−5) que aparecia com grade uniforme.
- **Limitador de saída / Overshoot Guard:** estimador de true-peak 4× com refinamento parabólico, mínimo móvel com hold e média (sem cliques), linkado entre canais. Com o guard desligado, ele substitui o clamp amostral, mantendo picos amostrais ≤ ceiling. Com o guard ligado, mantém TP ≤ ceiling − 0,15 dB. O clamp duro depois do decimador foi abandonado: num tom agudo clipado, a fundamental limitada em banda chega a 4/π do platô, e o clamp gerava aliasing de −23 dB.
- **CPU:** produto escalar vetorizado (8 acumuladores, FMA), linhas de atraso sem divisão inteira, ring duplicado no guard, coeficientes de saturação recalculados só quando mudam. No Alchemy, o histórico do FIR é guardado por fase (produto escalar contíguo), o filtro é pulado quando o Alchemy está desligado, e a raiz cúbica é um passo de Newton a partir da anterior. Desde 2026-10-18: rejeição do decimador de 100 dB (era 140), kernels simétricos dobrados, decimação meia-banda contígua, FIR do Alchemy com 1,2 ms. CLI, em ciclos (Classic + Gold 3 + Alchemy 1): High ~70, Extra ~195 Mciclos por s de áudio.
- **Anti-overshoot da referência:** ela faz um "ducking" de até −2 dB depois de bordas íngremes acima do teto. Pelo código embutido no binário (classe `OversamplingWithCeiling`), o ganho é decidido uma vez por buffer do host, a partir do pico amostral depois da decimação, e mantém o mínimo de dois buffers. Medido: 230 / 60 / 5 / 0 ms com buffers de 8192 / 2048 / 512 / 128. Não replicamos, porque depender do buffer do host não é um comportamento de conversor. Com buffer de 512 o loudness dos dois fica a ±0,05 LU.
- **Alchemy:** crossover 1/(1+(f/2965)⁴) de fase linear (FIR interpolado), lei k(nível, amount) tabelada a partir de platôs DC com uma sonda de 8 kHz, e suavização k = LPF(k³)^(1/3) com τ 0,4 ms, identificada com portadoras de 1,5 Hz a 1 kHz. Ganho médio em até 0,07 dB e bandas laterais em ~1 dB da referência. Sem detector separado e sem lookahead: a latência no High linear caiu de 585 para 374 amostras (328 depois do decimador de 100 dB).

<!-- AUTO:BEGIN -->
### Paridade (§7.1) — Sunbliss × referência, 48 kHz, qualidade High

| Teste | Critério | Resultado | OK |
|---|---|---|---|
| Curva estática DC, clip A (amplitude ≥ −6 dB) | ≤ 0,5 dB | máx 0.022 dB (média 0.003) | ✅ |
| Curva estática DC, clip B (amplitude ≥ −6 dB) | ≤ 0,5 dB | máx 0.000 dB (média 0.000) | ✅ |
| Curva estática DC, clip Hard (amplitude ≥ −6 dB) | ≤ 0,5 dB | máx 0.004 dB (média 0.003) | ✅ |
| Seno 1 kHz −12…+8 dB, clip A: RMS de saída | ≤ 0,5 dB | máx 0.015 dB | ✅ |
| Seno 1 kHz −12…+8 dB, clip B: RMS de saída | ≤ 0,5 dB | máx 0.090 dB | ✅ |
| Seno 1 kHz −12…+8 dB, clip Hard: RMS de saída | ≤ 0,5 dB | máx 0.025 dB | ✅ |
| Curva de saturação A, amounts 1…6 (x > −30 dB) | ≤ 0,5 dB | máx 0.143 dB | ✅ |
| Curva de saturação B, amounts 1…6 (x > −30 dB) | ≤ 0,5 dB | máx 0.206 dB | ✅ |
| Unity, curva A, amount 6 (nível baixo, ganho líquido) | mesmo efeito | ref +0.030 dB, nosso +0.000 dB | ✅ |
| Unity, curva B, amount 6 (nível baixo, ganho líquido) | mesmo efeito | ref -0.004 dB, nosso +0.000 dB | ✅ |
| Contour 2 dB: estático 8 kHz e curva de frequência | ≤ 0,5 dB | máx 0.173 dB | ✅ |
| Resposta linear filter_A (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.035 dB | ✅ |
| Resposta linear filter_B (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.075 dB | ✅ |
| Resposta linear filter_Hard (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.035 dB | ✅ |
| Resposta linear tone_A (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.006 dB | ✅ |
| Resposta linear tone_B (100 Hz…20 kHz) | ≤ 0,5 dB | máx 0.004 dB | ✅ |

#### Loudness na mixagem sintética (12 s)

| Ajuste | Ref LUFS | Nosso LUFS | Δ LU | Δ LU (guard on) | TP ref | TP nosso | TP nosso+guard |
|---|---|---|---|---|---|---|---|
| clipB_+6 | -13.03 | -13.08 | -0.05 | -0.07 | +1.14 | -0.00 | -0.15 |
| clipA_+6 | -12.90 | -12.95 | -0.05 | -0.08 | +0.26 | +0.00 | -0.14 |
| hard_+6 | -12.96 | -12.96 | -0.01 | -0.04 | +0.34 | +0.00 | -0.14 |
| hard_+10 | -9.48 | -9.46 | +0.02 | -0.04 | +0.62 | +0.01 | -0.14 |
| B_+6_satB3 | -10.45 | -10.46 | -0.01 | -0.05 | +0.56 | +0.00 | -0.14 |
| A_+6_satA6 | -10.78 | -10.77 | +0.01 | -0.03 | +0.41 | +0.00 | -0.14 |
| hard_+6_contour2 | -12.99 | -13.03 | -0.04 | -0.05 | +0.23 | +0.01 | -0.14 |
| B_+8_all | -8.30 | -8.32 | -0.02 | -0.05 | +1.15 | +0.01 | -0.14 |

### Superioridade (§7.2) — mesma grade, 48 kHz, High

| Métrica | Referência | Sunbliss | Vence? |
|---|---|---|---|
| Aliasing+ruído, tom 5 kHz +6 dB, clip A (dB rel. fund.) | -42.1 | -88.7 | ✅ |
| Aliasing+ruído, tom 9 kHz +6 dB, clip A (dB rel. fund.) | -53.7 | -88.1 | ✅ |
| Aliasing+ruído, tom 13 kHz +6 dB, clip A (dB rel. fund.) | -47.5 | -85.9 | ✅ |
| Aliasing+ruído, tom 15 kHz +6 dB, clip A (dB rel. fund.) | -45.9 | -77.2 | ✅ |
| Aliasing+ruído, tom 5 kHz +6 dB, clip B (dB rel. fund.) | 20.4 | -79.6 | ✅ |
| Aliasing+ruído, tom 9 kHz +6 dB, clip B (dB rel. fund.) | 16.8 | -84.9 | ✅ |
| Aliasing+ruído, tom 13 kHz +6 dB, clip B (dB rel. fund.) | 12.5 | -82.4 | ✅ |
| Aliasing+ruído, tom 15 kHz +6 dB, clip B (dB rel. fund.) | 17.6 | -80.7 | ✅ |
| Aliasing+ruído, tom 5 kHz +6 dB, clip Hard (dB rel. fund.) | -43.0 | -84.7 | ✅ |
| Aliasing+ruído, tom 9 kHz +6 dB, clip Hard (dB rel. fund.) | -86.2 | -87.7 | ✅ |
| Aliasing+ruído, tom 13 kHz +6 dB, clip Hard (dB rel. fund.) | -78.1 | -86.5 | ✅ |
| Aliasing+ruído, tom 15 kHz +6 dB, clip Hard (dB rel. fund.) | -77.8 | -77.0 | ❌ |
| Produtos < 18 kHz, 19+20 kHz +3 dB, clip A (dB) | -34.4 | -36.8 | ✅ |
| Produtos < 18 kHz, 19+20 kHz +3 dB, clip B (dB) | -34.5 | -37.1 | ✅ |
| Produtos < 18 kHz, 19+20 kHz +3 dB, clip Hard (dB) | -37.9 | -36.9 | ❌ |
| True-peak na mixagem, B_+3 (dBTP; nosso com guard) | +0.21 | -0.10 | ✅ |
|   loudness no mesmo ajuste (LUFS) | -15.92 | -15.97 | ✅ |
| True-peak na mixagem, B_+6 (dBTP; nosso com guard) | +1.14 | -0.15 | ✅ |
|   loudness no mesmo ajuste (LUFS) | -13.03 | -13.11 | ✅ |
| True-peak na mixagem, B_+10 (dBTP; nosso com guard) | +0.74 | -0.14 | ✅ |
|   loudness no mesmo ajuste (LUFS) | -9.68 | -9.62 | ✅ |
| Resíduo abaixo do teto (−6 dB), clip A, banda 20 kHz (dB rms) | -94.5 | -107.9 | ✅ |
| Resíduo abaixo do teto (−6 dB), clip Hard, banda 20 kHz (dB rms) | -94.5 | -107.9 | ✅ |
| Resíduo abaixo do teto (−6 dB), clip B, banda 20 kHz (dB rms) | -70.8 | -70.7 | ✅ |
| Tudo neutro: resíduo de pico (dBFS) | -40.8 | -400.0 | ✅ |
| Recuperação pós-rajada +12 dB, clip Hard: erro do sinal de prova (dB) | 0.001 | 0.006 | ❌ |
| Recuperação pós-rajada +12 dB, clip B: erro do sinal de prova (dB) | 0.090 | 0.094 | ❌ |
| Clique na troca 'mode B->Hard': excesso de HF > 16 kHz em ±5 ms (dB) | +0.01 | -0.18 | ✅ |
| Clique na troca 'curve A->B (sat 6)': excesso de HF > 16 kHz em ±5 ms (dB) | -2.63 | -2.40 | ✅ |
| Clique na troca 'clip on->off': excesso de HF > 16 kHz em ±5 ms (dB) | -0.16 | -2.03 | ✅ |
| Clique na troca 'tone Flat->A': excesso de HF > 16 kHz em ±5 ms (dB) | -0.00 | -0.00 | ✅ |
| Clique na troca 'bypass on': excesso de HF > 16 kHz em ±5 ms (dB) | -0.67 | -1.99 | ✅ |
| CPU, mesmo host (pedalboard, buffer 512), High, estéreo 48 kHz: Mciclos por s de áudio | 113 | 68 | ✅ |
|   ≈ % de um núcleo a 4 GHz | 2.83 | 1.70 | — |
| CPU Pristine / Extra, Mciclos por s (nosso; a referência não expõe a troca) | — | 110 / 195 | — |
<!-- AUTO:END -->

## O que falta

- **Validação em DAWs** (Ableton, Reaper, Logic/Pro Tools): os plugins estão instalados em `~/Library/Audio/Plug-Ins/{CLAP,VST3,Components}`. Falta abrir em cada host (o teste precisa ser feito pelo usuário). pluginval e clap-validator não estão instalados.
- **Fase mínima:** falta recapturar "Extra Pristine + Minimum": nas duas tentativas saiu idêntico ao High Minimum, inclusive no aliasing.
- **High + Min Phase:** a referência tem menos aliasing (−97 contra −83 dB a 9 kHz). Os nossos componentes são aliases discretos da etapa de clip (harmônicos 19 a 23 de 9 kHz dobrando a 192 kHz). Não vêm do decimador (mais precisão na transformação para fase mínima não muda nada) nem do limitador (desligá-lo não muda nada). Segundo o binário, o modo mínimo dela usa meia-banda IIR polifásica. Por que isso reduz os aliases da etapa de clip ainda está em aberto.
- **Ressalvas de superioridade:** IMD 19+20 kHz no Hard (−36,9 contra −37,9 dB) e aliasing no Hard a 15 kHz (−77,0 contra −77,8 dB). As duas diferenças são ≤ 1 dB.
- **CPU:** resolvido. A medição antiga (tempo de relógio de um render de 20 s) chegou a apontar 4,3 % contra 2,7 %. Medindo ciclos do laço de render (`lab/cpu_probe.py`), o Sunbliss gasta 68 Mciclos por s de áudio e a referência 113. Maior custo restante: Alchemy, ~12 Mciclos/s sobre um caminho de clip de ~58.
- **Polinômios não identificados:** o binário tem mais quatro polinômios de grau 15 que não correspondem a nenhum modo medido.
- **F7:** pares nivelados e ABX. Precisa de música real em `refs/music/` (não baixamos música).

## Modos de oversampling: referência × Sunbliss (48 kHz)

Estados da referência capturados na GUI dela (menus REALTIME e tipo de fase) e medidos com todo o resto neutro.

Dados atualizados (buffer de 512, código de 2026-10-18) em `docs/data/os_modes.json`, gerado por `lab/os_modes.py`. Atrasos de grupo da medição original (`docs/data/gc_oversampling_modes.json`).

| Modo | Latência | Plano até | Atraso de grupo extra em 10 / 15 / 20 kHz | Aliasing Hard, tom 9/13/15 kHz +6 dB |
|---|---|---|---|---|
| Ref. High Linear | 764 | 20 kHz (−4,6 dB em 23,5 kHz) | 0 / 0 / 0 | −86 / −78 / −78 |
| Ref. Pristine Linear | 769 | idem | 0 / 0 / 0 | −98 / −81 / −98 |
| Ref. Extra Pristine Linear | 771 | idem | 0 / 0 / 0 | −98 / −81 / −97 |
| Ref. High Minimum | 711 | 23 kHz | +0,8 / +2,4 / +6,8 | −97 / −81 / −95 |
| Ref. Pristine Minimum | 711 | 23 kHz | idem | −101 / −75 / −96 |
| Sunbliss High Linear | **328** | 20 kHz | 0 / 0 / 0 | −88 / −87 / −77 |
| Sunbliss Pristine Linear | 330 | 20 kHz | 0 / 0 / 0 | −91 / −94 / −91 |
| Sunbliss High Min Phase | **250** | 22 kHz | +1,2 / +3,8 / +14 | −83 / −83 / −78 |
| Sunbliss Pristine Min Phase | 251 | 22 kHz | +1,3 / +3,9 / +14 | −100 / −94 / −108 |

Leitura:
- Na referência, quase toda a latência (~700 amostras) vem do lookahead do ganho de compensação por buffer (`OversamplingWithCeiling`, segundo o binário). A fase mínima dela economiza só ~55 amostras e muda o *formato* do transiente (sem pré-ringing).
- A fase mínima dela é suave (passa até 23 kHz, pouca rotação). A nossa rejeita tudo a partir de fs/2 (sem aliasing na banda). Para tirar a rotação da banda audível, no modo Min Phase a borda vai a 22 kHz e a rejeição a 100 dB (Hilbert: a fase de um filtro de fase mínima cresce com a profundidade da banda de rejeição). A rotação em 20 kHz fica em 0,29 ms contra 0,14 ms dela; as duas abaixo do audível.
- **Ressalva:** no High + Min Phase, o aliasing da referência é melhor (−97 contra −83 dB em 9 kHz). Curiosamente o modo mínimo dela tem menos aliasing que o linear dela no High, o que sugere processamento interno diferente nesse modo. Ver "O que falta".
