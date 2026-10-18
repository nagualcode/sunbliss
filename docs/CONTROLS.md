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
| `clipper_mode` | Clipper Mode (`clipper_mode`) | Classic / Modern / Hard | Classic | sim (Classic sem a inversão de polaridade em drive extremo) |
| `clipper_filter_on_off` | Clip Filter (`clip_filter`) | on/off | off | sim |
| `clip_guard` | Clip Guard (`clip_guard`) | on/off | off | **diferente**: true-peak ≤ ceiling |
| `gold_on_off` | Gold On (`gold_on`) | on/off | on | sim |
| `gold` | Gold (`gold`) | 0 … 6 dB | 0 | sim |
| `gold_mode` | Gold Mode (`gold_mode`) | Classic / Modern | Modern | Classic = equação do manual da Lavry |
| `unity` | Unity Gain (`unity_gain`) | on/off | off | sim |
| `alchemy_on_off` | Alchemy On (`alchemy_on`) | on/off | on | sim |
| `alchemy` | Alchemy (`alchemy`) | 0 … 2 dB | 0 | sim (lei, crossover e dinâmica medidos) |
| `box_tone_mode` | Box Tone (`box_tone`) | Flat / Classic / Modern | Flat | sim (Flat bit-transparente) |
| `wet` / `dry` | Wet / Dry | 0 … 100 % | 100 / 0 | sim |
| `dry_wet_link` | Dry Wet Link (`dry_wet_link`) | on/off | on | sim |
| `wet_mute` / `dry_mute` | Wet Mute / Dry Mute | on/off | off | sim |
| `delta_phase` | Delta (`delta`) | on/off | off | **diferente**: desconta o Input Trim |
| `bypass` | Bypass (`bypass`) | on/off | off | sim |
| *(configuração oculta)* | Oversampling (`oversampling`) | High / Pristine / Extra Pristine | High | mesmos fatores |
| *(configuração oculta)* | Offline Oversampling (`offline_oversampling`) | Same / High / Pristine / Extra Pristine | Same | sim |
| *(configuração oculta)* Linear/Minimum Phase | Oversampling Type (`oversampling_type`) | Linear Phase / Minimum Phase | Linear | a calibrar contra a captura dos modos da referência |
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
- **Converter B (Classic):** compressão suave e **assimétrica** desde níveis baixos: H2 dominante, THD −68 dB a −30 dBFS, subindo 1 dB/dB. Curva DC: +0,5 → 0,4839, −0,5 → −0,4915, +1,0 → 0,943, saturando em ~0,97. Em |u| ≤ 1,31 c a curva é **um polinômio de grau 9** sem termo constante: o ajuste por mínimos quadrados a 262 platôs DC deixa resíduo de **1,4e−7**, o piso do f32 da E/S (`lab/fit_classic.py`, `docs/data/fit_classic_poly.json`). Graus 5 e 7 deixam 5e−2 e 3e−2, e o 11 não melhora. **Memória de sobrecarga:** acima de **1,32 c** (a borda do polinômio) a saída decai para zero com **τ = 72,5 ms** desde o início da sobrecarga (degraus de 1,33 a 2,0 c: 0,817 → 0,602 → 0,426 … a cada 25 ms). É isso que faz os platôs de grave "afundarem". Com |x| ≳ 2,5 c a referência começa o decaimento **com a polaridade invertida** (DC de 3,0 → −0,83). **Decisão:** replicar o polinômio, a borda e o decaimento τ72 como a referência. Não replicar a inversão de polaridade.
- Em todos os modos, o **pico amostral** final fica preso em *c*. O true-peak passa até **+1,04 dB** (44,1/48 kHz).

### Clip Filter
- É um passa-baixas de 1 polo, fc ≈ **34 kHz** (fixo em Hz: idêntico a 48 e 96 kHz), colocado **antes** do clipper. Prova: os harmônicos do clip Hard a +12 dB não mudam com o filtro (Δ ≤ 0,01 dB). No modo B o efeito é duplicado (−2,58 dB a 20 kHz contra −1,29 dB nos outros), e o filtro estabiliza o modo B em drive alto.

### Overshoot Guard
- Na referência, o `clip_guard` quase não muda nada: −0,001 dB de pico, true-peak igual. O que limita o pico é (1) um clamp amostral sempre ativo e (2) um **ducking por bloco** de até −2 dB, disparado por bordas íngremes acima do teto (degraus de DC 2,0 e 4,0 → −1 e −2 dB). A decisão é tomada **uma vez por buffer do host** e vale por dois buffers, então a duração depende do host: **230 ms com buffer de 8192** (o padrão do pedalboard, que deu o "hold de 0,4 s" medido nas fases F1–F6), 60 ms com 2048, **5 ms com 512**, nenhum com 128. Desde 2026-10-17 o lab mede a referência com buffer de 512 (`lab/goldclip.py`, `BUFFER`), como num DAW típico. Com isso, a diferença de loudness em drive pesado cai de +0,5 LU para ±0,1 LU.
- **Sunbliss, guard desligado:** um limitador suave de lookahead (1 ms + hold de 16 amostras) mantém os picos amostrais ≤ ceiling. Ele usa a estimativa de true-peak, que é constante em tons estáveis (os picos amostrais "batem" contra fs e modulariam o ganho). Substitui o clamp duro, que causava aliasing forte num tom agudo clipado, cuja fundamental limitada em banda chega a 4/π do platô. O clamp continua como rede de segurança.
- **Sunbliss, guard ligado:** o mesmo limitador controla o **true-peak** com margem de 0,15 dB. Estimador 4× com refinamento parabólico, sem cliques, linkado entre canais. Resultado: TP ≤ −0,11 dBTP em todos os ajustes medidos.

### Saturation (Gold) — `docs/data/gc_gold_curves.json`
- É aplicada **depois** do clipper, por amostra, sem memória (histerese < 1,5e−4).
- **Curve B (Classic):** ganho = knob (0,5 → +0,49 dB … 6 → +6,00 dB) abaixo de T = −2·knob dBFS. Acima de T vem uma curva exponencial até (c, c). Ela coincide com a generalização da Soft Saturation da Lavry. O ajuste da exponencial com fechamento (b), y(c) = c, dá erro ≤ 0,21 dB contra a referência e **rms 0,03 dB contra o gráfico do manual** (`docs/data/lavry_fit.json`). Decisão: curva B = equação do manual, fechamento (b).
- **Curve A (Modern):** linear até **x = 0,5 c** e depois uma **quadrática C1** até (c, c), com inclinação final 4 − 3g (erro ≤ 0,06 dB, `docs/data/fit_modern_hermite.json`). A concavidade limita g ≤ 4/3, o que explica o "máximo de +2,5 dB". O mapeamento knob → g é linear por partes: 0 → 0; 2 → 1,32 dB; 6 → 2,50 dB.
- **Unity:** multiplica por 1/g (ganho de pequeno sinal) depois de todo o processamento wet. Medido: Classic 6 → −6,00 dB; Modern 6 → −2,53 dB.

### Contour (Alchemy) — `docs/data/gc_alchemy*.json`, `alchemy_mod.json`
Estrutura: y = banda baixa + k · banda alta, com k ≤ 1 vindo do nível banda larga |x|/c do sinal já clipado. Cada parte foi identificada por caixa-preta:
- **Crossover:** banda baixa = 1/(1 + (f/fc)⁴) com **fc = 2965 Hz**, de fase linear (um Linkwitz-Riley de fase zero), e banda alta = entrada − banda baixa. Esse modelo explica a forma em frequência medida com erro de 0,0024.
- **Lei estática k(nível, amount):** sonda de 8 kHz pequena sobre platôs DC de −20 a 0 dB rel. c, amounts de 0,25 a 2 (`lab/probe_alchemy_inst.py`). O joelho desce com o amount: ≈ −1,5 dB rel. c com 0,25, −3 com 0,5, −5,7 com 1, −8,5 com 2. Com 2, k chega a −4,1 dB em −0,5 dB rel. c. Nenhuma forma fechada simples explica a lei (nem a curva da Lavry, nem uma reta em amplitude), então ela é tabelada.
- **Dinâmica:** portadoras de 1,5 Hz a 1 kHz, sem clip, mais uma sonda em 15 kHz (`lab/probe_alchemy_mod.py`). Abaixo de ~100 Hz o ganho segue a forma de onda (bandas laterais em 15 kHz ± 2f a −18 dB com A = −1 dB). Acima disso, as bandas laterais caem 6 dB/oitava, e o ganho médio migra da média da lei para a **média cúbica**, com 0,3 dB a menos de redução. O modelo k = LPF(k_inst³)^(1/3), com um polo e τ = 0,4 ms, reproduz tudo: ganho médio em até 0,07 dB e bandas laterais em ~1 dB.
- **Consequência:** um grave que bate no teto modula os pratos com a própria forma de onda (bandas laterais de AM), mas um tom agudo não se distorce (H3 a −46 dB num seno de 4 kHz a 0 dB, contra −22 dB de uma lei realmente instantânea).
- **Sunbliss:** o mesmo modelo, na taxa de oversampling. A banda baixa é um FIR interpolado: taps de 1,6 ms projetados na taxa base, espaçados de M amostras, e dois boxcars de comprimento M para anular as imagens. Sem detector e sem lookahead.
- **No binário** (só leitura de nomes, sem decifrar nada): o parâmetro se chama `mx_amount` e o processador `MXProcessor`, com um `LinearPhaseCrossover` de protótipo SVF passa-baixas. O manual do AD122-96 **MX** diz que a diferença do MX em relação ao MkIII é o clipping analógico de entrada, com o mesmo software. O Alchemy seria então o modelo do comportamento analógico do MX perto do clip.

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

## Converter: recursos do hardware original que a referência não tem

Fonte: manual do AD122-96 MkIII (Parte I e Apêndice I). As opções usam os nomes do manual.

| Parâmetro | Valores | Padrão | O que faz |
|---|---|---|---|
| Polarity Invert | on/off | off | Inversão de polaridade digital, como no hardware. Crossfade passando por zero, sem clique. |
| DC Removal | on/off | off | HPF de 1 polo em 0,3 Hz (o manual diz que assenta em < 3 s). Atua antes do clipper, então DC não desloca o clip. |
| Word Length | Float / 24 / 22 / 20 / 18 / 16 bit | Float | Redução de palavra no último estágio. |
| Dither | Off / HPDF / Flat | HPDF | HPDF = "triangle high pass" do manual: número uniforme novo menos o anterior. Flat = TPDF plano. Só atua com Word Length ≠ Float. |
| Noise Shaping | Off / NS1–NS4 | Off | Realimentação de erro de 24ª ordem, fase mínima. Curvas traçadas das Figs. 11–12 do manual: vales em ~4 kHz e ~12 kHz, ruído empurrado acima de 14 kHz. NS1 é a mais suave. Desligado quando o Dither está em Off, como no hardware. |
| Data Limit | on/off | off | Fundo de escala limitado a 1 LSB abaixo do máximo, para nenhum medidor seguinte acusar "over". |

Medição (44,1 kHz, 16 bit, silêncio na entrada; dB relativos ao ruído só com HPDF):

| | 1 kHz | 4 kHz | 8 kHz | 12 kHz | 16 kHz | 20 kHz | rms |
|---|---|---|---|---|---|---|---|
| NS1 | −2,7 | −4,6 | −2,9 | −4,0 | +2,8 | +6,4 | −93,5 dBFS |
| NS2 | −6,3 | −9,6 | −4,7 | −6,4 | +6,8 | +11,6 | −88,5 dBFS |
| NS3 | −7,7 | −12,1 | −6,1 | −11,4 | +10,2 | +19,8 | −81,6 dBFS |
| NS4 | −9,6 | −20,8 | −10,5 | −10,1 | +14,2 | +29,4 | −72,6 dBFS |

Os alvos são leituras de um gráfico impresso: a forma é fiel, mas não temos os coeficientes originais.

Achado importante: o manual especifica THD+N de 0,0008% a −6 dBFS e 0,0012% em fundo de escala. O conversor real é praticamente transparente abaixo do clip. A distorção de baixo nível do "Classic" da referência (~0,5% a −9 dBFS) é coloração dela, não do hardware. Os modos Modern e Hard do Sunbliss (THD < −170 dB abaixo do teto) são os fiéis ao hardware nesse aspecto.
