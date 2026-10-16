# Tone A/B (equivalente ao Box Tone)

## Medição da referência

Ruído a −40 dBFS, função de transferência H1 (Welch), resultado relativo ao
caminho neutro da referência (`lab/probe_linear.py`, dados em
`docs/data/gc_linear_{44100,48000,96000}.json`).

| f | A @44,1 | A @48 | A @96 | B @44,1 | B @48 | B @96 |
|---|---|---|---|---|---|---|
| 1 kHz | −0,08 | −0,07 | −0,07 | +0,02 | +0,04 | +0,02 |
| 3 kHz | −0,21 | −0,17 | −0,17 | +0,03 | +0,06 | +0,03 |
| 8 kHz | −0,21 | −0,18 | −0,15 | −0,02 | +0,02 | +0,02 |
| 16 kHz | −0,43 | −0,38 | −0,25 | −0,23 | −0,17 | −0,05 |
| 20 kHz | −1,75 | −0,44 | −0,28 | −1,66 | −0,32 | −0,12 |
| 22 kHz | — | −2,58 | −0,37 | — | −2,49 | −0,17 |
| 40 kHz | — | — | −1,86 | — | — | −1,68 |

Leitura: há um contorno suave abaixo de 20 kHz. A ("modern") tem uma leve
depressão de 0,2 dB entre 2 e 10 kHz e queda até −0,45 dB em 18 kHz. B
("classic") tem um brilho de +0,03 a +0,06 dB entre 1 e 6 kHz e queda acima de
10 kHz. Por cima vem um passa-baixas "hipersônico" que depende da taxa: íngreme
em ~0,45·fs a 44,1/48 kHz e suave (≈ 2ª ordem, −1,7 dB em 40 kHz) a 96 kHz.

## Implementação

- FIR de fase linear, 255 taps na taxa base (atraso 127, sempre presente para manter a latência constante). Projeto por amostragem em frequência com janela de Kaiser (β para 70 dB).
- As curvas-alvo são as medidas, em 1/6 de oitava abaixo de 16 kHz e 1/24 acima (`crates/core/src/tone_tables.rs`, gerado pelo lab). Há uma família por taxa: 44,1, 48 e 96 kHz. Em 88,2 kHz, a parte acima de 20 kHz é reescalada para a nova Nyquist.
- Abaixo de 350 Hz a curva é forçada a 0 dB, então o Tone só atua de fato acima de ~400 Hz.
- **Flat** é atraso puro, bit-transparente.
- A troca de modo faz crossfade de 10 ms entre as saídas.

## Diferença em relação à referência

Erro máximo entre 100 Hz e 20 kHz, a 48 kHz: **0,006 dB (A)** e **0,004 dB (B)**
(`docs/data/parity.json` → `linear`). Não alteramos o contorno. A única
melhoria é estrutural: o Flat é exatamente transparente, enquanto o caminho
neutro da referência não é.
