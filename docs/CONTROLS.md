# Controles: mapeamento e semântica (F1)

Para estudo pessoal (não distribuído), os rótulos do Sunbliss **espelham os da
referência**: mesmos nomes, mesma ordem nas opções e mesmos padrões. Assim, um
ajuste pode ser copiado de um plugin para o outro diretamente. O nome do
plugin continua "Sunbliss". Nos controles que funcionam diferente, a GUI mostra
um tooltip explicando a diferença.

Fonte do inventário: `docs/goldclip_params.json` (Gold Clip 1.5.0, AU, 23
parâmetros). Medições: `lab/probe_*.py`, com dados em `docs/data/`.

Nomes internos no código do núcleo e no laboratório: Clipper Classic = modelo
"B", Modern = "A" (vale também para Gold Mode e Box Tone).

## Tabela de parâmetros

| Referência (param id) | Sunbliss: nome no host (campo) | Valores / faixa | Padrão | Igual? |
|---|---|---|---|---|
| `input_gain` | Input Trim (`input_trim`) | −12 … +12 dB | 0 | sim |
| `output_gain` | Output Trim (`output_trim`) | −12 … +12 dB | 0 | sim |
| `gain_link` | Trim Link (`trim_link`) | on/off | on | **diferente**: também vale para automação |
| *(só GUI)* | Trim Link Offset (`trim_link_offset`) | −12 … +12 dB | 0 | parâmetro de verdade |
| `ceiling` | Clip Ceiling (`clip_ceiling`) | −36 … 0 dB | 0 | sim (sem o ducking) |
| `clipper_on_off` | Clipper (`clipper`) | on/off | on | sim |
| `clipper_mode` | Clipper Mode (`clipper_mode`) | Classic / Modern / Hard | Classic | sim (Classic sem o colapso em drive extremo) |
| `clipper_filter_on_off` | Clip Filter (`clip_filter`) | on/off | off | sim |
| `clip_guard` | Clip Guard (`clip_guard`) | on/off | off | **diferente**: true-peak ≤ ceiling |
| `gold_on_off` | Gold On (`gold_on`) | on/off | on | sim |
| `gold` | Gold (`gold`) | 0 … 6 dB | 0 | sim |
| `gold_mode` | Gold Mode (`gold_mode`) | Classic / Modern | Modern | Classic = equação do manual da Lavry |
| `unity` | Unity Gain (`unity_gain`) | on/off | off | sim |
| `alchemy_on_off` | Alchemy On (`alchemy_on`) | on/off | on | sim |
| `alchemy` | Alchemy (`alchemy`) | 0 … 2 dB | 0 | **diferente**: envelope de pico, sem AM |
| `box_tone_mode` | Box Tone (`box_tone`) | Flat / Classic / Modern | Flat | sim (Flat bit-transparente) |
| `wet` / `dry` | Wet / Dry | 0 … 100 % | 100 / 0 | sim |
| `dry_wet_link` | Dry Wet Link (`dry_wet_link`) | on/off | on | sim |
| `wet_mute` / `dry_mute` | Wet Mute / Dry Mute | on/off | off | sim |
| `delta_phase` | Delta (`delta`) | on/off | off | **diferente**: desconta o Input Trim |
| `bypass` | Bypass (`bypass`) | on/off | off | sim |
| *(configuração oculta)* | Oversampling (`oversampling`) | High / Pristine / Extra Pristine | High | mesmos fatores |
| *(configuração oculta)* | Offline Oversampling (`offline_oversampling`) | Same / High / Pristine / Extra Pristine | Same | sim |
| *(medidor)* Clip Clock | Clip Clock: MAX CLIP + MIN:SEC (só leitura) | — | — | sim |
| A/B | A/B (estado do editor) | dois snapshots | — | sim |

Nenhum parâmetro do JSON ficou sem equivalente.

## Semântica medida na referência (hipóteses que serão implementadas)

Todos os números são de 48 kHz, salvo indicação.

### Trims e Link
- O Input multiplica antes de tudo, exceto o Tone, que vem antes do Input (§CHAIN). O Output multiplica depois do mixer: afeta dry e wet (medido: dry-only + Output +6 → +6,0 dB).
- **Link:** Input +x ⇒ Output −x + offset. Na referência, o link vive só na GUI. No Sunbliss, o link é aplicado no DSP: `out_eff = link ? (−input + link_offset) : output`. Assim a automação e o host enxergam o mesmo comportamento.

### Ceiling
- Tudo se normaliza pelo ceiling *c*: o clipper limita em *c*, e a saturação e o contour usam `x/c` (medido: curva Gold a −6 dB de ceiling, normalizada, difere 8e−5 da curva a 0 dB).
- Com o clipper **desligado**, a referência ignora o ceiling na saturação. Nós replicamos: c = 1 quando Clip = off.

### Clip Mode (medido com DC e senos; ver `docs/data/gc_dc_*.json`, `gc_levels_*.json`)
- **Hard:** linear até o teto. O platô fica em 0,97–0,99 (o "headroom" vem do anti-overshoot). THD < −90 dB abaixo do teto.
- **Converter A (Modern):** linear até ~0,9 e joelho curto até um teto **0,9798** (−0,177 dB). Seno a 0 dBFS → pico −0,12 dB.
- **Converter B (Classic):** compressão suave e **assimétrica** desde níveis baixos: H2 dominante, THD −68 dB a −30 dBFS, subindo 1 dB/dB. Curva DC: +0,5 → 0,4839, −0,5 → −0,4915, +1,0 → 0,943, saturando em ~0,97. **Memória de sobrecarga:** com |x| > ~1,33 c, a saída decai com τ ≈ 72 ms (é isso que faz os platôs de grave "afundarem"). Com |x| ≳ 4 c e em agudos acima de ~+12 dB, a referência **diverge**: a polaridade inverte e a saída trava em −1 (inarmônico de +1 a +37 dB rel. fundamental). **Decisão:** replicar a curva estática, a assimetria e a memória de sobrecarga. **Não** replicar a divergência: é um defeito, não um som de conversor.
- Em todos os modos, o **pico amostral** final fica preso em *c*. O true-peak passa até **+1,04 dB** (44,1/48 kHz).

### Clip Filter
- É um passa-baixas de 1 polo, fc ≈ **34 kHz** (fixo em Hz: idêntico a 48 e 96 kHz), colocado **antes** do clipper. Prova: os harmônicos do clip Hard a +12 dB não mudam com o filtro (Δ ≤ 0,01 dB). No modo B o efeito é duplicado (−2,58 dB a 20 kHz contra −1,29 dB nos outros), e o filtro estabiliza o modo B em drive alto.

### Overshoot Guard
- Na referência, o `clip_guard` quase não muda nada: −0,001 dB de pico, true-peak igual. O que limita o pico é (1) um clamp amostral sempre ativo e (2) um **ducking adaptativo** de até −2 dB, com hold de ~0,4 s, disparado por bordas íngremes acima do teto (medido com degraus de DC 2,0 e 4,0 → −1 e −2 dB). O ducking explica por que a referência sai mais baixa em drive alto.
- **Sunbliss, guard desligado:** um limitador suave de lookahead (1 ms + hold de 16 amostras) mantém os picos amostrais ≤ ceiling. Ele usa a estimativa de true-peak, que é constante em tons estáveis (os picos amostrais "batem" contra fs e modulariam o ganho). Substitui o clamp duro, que causava aliasing forte num tom agudo clipado, cuja fundamental limitada em banda chega a 4/π do platô. O clamp continua como rede de segurança.
- **Sunbliss, guard ligado:** o mesmo limitador controla o **true-peak** com margem de 0,15 dB. Estimador 4× com refinamento parabólico, sem cliques, linkado entre canais. Resultado: TP ≤ −0,11 dBTP em todos os ajustes medidos.

### Saturation (Gold) — `docs/data/gc_gold_curves.json`
- É aplicada **depois** do clipper, por amostra, sem memória (histerese < 1,5e−4).
- **Curve B (Classic):** ganho = knob (0,5 → +0,49 dB … 6 → +6,00 dB) abaixo de T = −2·knob dBFS. Acima de T vem uma curva exponencial até (c, c). Ela coincide com a generalização da Soft Saturation da Lavry. O ajuste da exponencial com fechamento (b), y(c) = c, dá erro ≤ 0,21 dB contra a referência e **rms 0,03 dB contra o gráfico do manual** (`docs/data/lavry_fit.json`). Decisão: curva B = equação do manual, fechamento (b).
- **Curve A (Modern):** linear até **x = 0,5 c** e depois uma **quadrática C1** até (c, c), com inclinação final 4 − 3g (erro ≤ 0,06 dB, `docs/data/fit_modern_hermite.json`). A concavidade limita g ≤ 4/3, o que explica o "máximo de +2,5 dB". O mapeamento knob → g é linear por partes: 0 → 0; 2 → 1,32 dB; 6 → 2,50 dB.
- **Unity:** multiplica por 1/g (ganho de pequeno sinal) depois de todo o processamento wet. Medido: Classic 6 → −6,00 dB; Modern 6 → −2,53 dB.

### Contour (Alchemy) — `docs/data/gc_alchemy*.json`
- O detector é **banda larga e instantâneo** (amostra a amostra, sem attack/release) sobre |x|/c. A ação é um shelf de agudos complementar: −0,02 dB em 1 kHz, −0,29 em 2 kHz, −0,9 em 3 kHz, −1,41 em 4 kHz, −1,86 em 8 kHz (com 2 dB, tom a −0,5 dBFS).
- Estático (tom de 8 kHz, amount 2): começa em ≈ −9 dB rel. c; −0,5 dB em −6; −2,0 dB em 0.
- Lei instantânea: o joelho desce com o amount (≈ −4,5 dB rel. c com 0,5; −8 com 1; −12 com 2). Redução instantânea de até −4,1 dB.
- **Consequência:** na referência, a banda alta é modulada pela forma de onda inteira, e isso gera bandas laterais de AM (8 kHz ± 120 Hz com um grave de 60 Hz).
- **Sunbliss:** mesma lei média, mas aplicada sobre um envelope de pico **suavizado e simétrico**, usando o lookahead já existente. O objetivo é a mesma redução média com bandas laterais muito menores. Isso será medido.

### Tone (Box Tone) — `docs/data/gc_linear_*.json`
- Fica **antes** do Input e do clipper. Prova: o harmônico em 21 kHz do clip não muda com o Tone.
- É linear. Contorno de décimos de dB mais um passa-baixas "hipersônico". A e B estão detalhados em `docs/TONE.md` (F4).

### Mixer, Delta, Bypass
- Wet/Dry em **amplitude linear**: wet 50 % → −6,02 dB. Com Mix Link, dry = 100 − wet.
- O dry é cópia da entrada do plugin (antes do Tone e do Input), atrasada pela latência.
- **Delta (ref.):** (wet − entrada) × output. Não compensa o Input trim: com Input +3 o delta contém o próprio ganho. **Sunbliss:** delta = (wet − entrada·g_in) × output, ou seja, ouve só o que o processamento muda. É divergência deliberada.
- **Bypass:** bit-exato, alinhado à latência (null −145 dB na ref.).

### Clip Meter (Clip Clock)
- Não é parâmetro. Mostra o máximo de clipping (dB acima do ceiling, medido na entrada do clipper) e o tempo mm:ss da transporte onde ocorreu. Reset por duplo clique. No Sunbliss: medidor read-only + timestamp via `TransportInfo`.

### Oversampling
- High 4× / Pristine 8× / Extra 16× a 44,1/48 kHz; metade disso a 88,2/96 kHz; um quarto acima disso.
- Sunbliss: o decimador de conversor é plano até max(20 kHz; 0,4167·fs), limitado a 0,4535·fs (20 kHz a 44,1/48 kHz; 40 kHz a 96 kHz), com rejeição de 140 dB a partir de fs/2.
- Render Quality permite usar outra qualidade só no render offline. A latência reportada acompanha a troca.
- Latência medida da referência (High, fase linear): 764 amostras (44,1/48 kHz) e 1394 (88,2/96 kHz).
- O caminho neutro da referência é plano até 20 kHz, com −6 dB em fs/2 (o último decimador é meia-banda centrado em Nyquist, o que deixa entrar aliasing na faixa 20–24 kHz).
