# Ambiente (F0)

Registrado em 2026-10-16.

| Item | Valor |
|---|---|
| Máquina | Apple M5, macOS 27.0.1 |
| Rust | rustc 1.99.0 (truce pede ≥ 1.92) |
| cargo-truce / truce | 6.3.0 (registry + checkout git `2579127`) |
| `cargo truce doctor` | OK: Xcode CLI, codesign, auval, targets x86_64+aarch64. Avisos: pluginval e clap-validator ausentes; sem SDK AAX. Faltava `truce.toml` (criado na F5). |
| Python do laboratório | 3.11.16 em `.venv/` (pedalboard 0.9.26, numpy, scipy, soundfile, matplotlib, pyloudnorm, pymupdf, pyyaml) |
| Referência | Gold Clip **1.5.0**, apenas **AU**: `/Library/Audio/Plug-Ins/Components/Gold Clip.component` (também há `Gold Clip Track.component`). Não há VST3 instalado. |
| Manuais | `refs/manual/lavry-ad122-96mkiii-manual.pdf`, `refs/manual/GoldClip.pdf` (+ textos extraídos `.txt`). **O manual da MX não está na máquina.** |
| Outros | `refs/lavry-45-second-test-tones/` (tons de teste da Lavry) |

## Teste de fumaça

`pedalboard.load_plugin(GC_PATH)` carrega o AU headless, sem bloqueio de licença.
Renderização de 1 s de seno OK.

## Particularidades da referência (importantes para o laboratório)

1. **Latência reportada = 0** ao pedalboard. A latência real, medida pelo pico do
   IR, é de **764 amostras a 44,1/48 kHz** e **1394 a 88,2/96 kHz** (oversampling
   padrão "High", fase linear). O helper `lab/goldclip.py` compensa isso.
2. **Aquecimento:** depois de `reset()`, os primeiros ~10 mil amostras a 48 kHz saem
   com configuração velha (IR com pico 0,762 em vez de 0,979). Todo render recebe
   32768 amostras de silêncio na frente, que depois são descartadas.
3. **Oversampling e fase não são parâmetros expostos.** Eles ficam no estado do
   plugin, que é um blob ofuscado (cabeçalho `KLBW`). Não tentamos decifrá-lo.
   Todas as medições usam o padrão (High, Linear Phase), que é o que aparece na
   captura de tela do manual.
4. **Trim Link** e **Trim Link Offset** existem só na GUI: mover `input_gain` pela
   API não move `output_gain`.

## Reprodução

```sh
python3.11 -m venv .venv && .venv/bin/pip install pedalboard numpy scipy soundfile matplotlib pyloudnorm pymupdf pyyaml
cd lab && ../.venv/bin/python probe_levels.py 48000
```

## Validação de formatos (F5)

- `make install` → CLAP, VST3 e AU v2 em `~/Library/Audio/Plug-Ins/`.
- `auval -v aufx Snbl JSon` → **AU VALIDATION SUCCEEDED**. O `cargo truce validate --auval` chama `auval -a`, que trava varrendo AUs de terceiros nesta máquina; use o comando direto.
- `lab/plugin_check.py {vst3,au}` → o binário produz o mesmo áudio que o CLI (pior caso −140 dBFS, limite do f32 na E/S do host).
- Observação sobre o pedalboard: ele quantiza floats na própria grade e interpreta `%` como 0–100, então o script define valores normalizados exatos. Ele também lê a latência de uma preparação anterior ao trocar a taxa de amostragem.
