//! WAV -> WAV (or raw f64) renderer for the lab and CI.
//!
//! sunbliss-cli IN OUT [--params p.json] [--set key=value ...] [--block N]
//!          [--compensate] [--rate HZ --channels N (raw input)] [--latency]
//!
//! Files ending in `.f64` are raw little-endian interleaved f64; `--rate` and
//! `--channels` describe raw input. WAV output is 32-bit float.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use sunbliss_core::{ClipMode, Curve, Engine, Params, Quality, ToneMode};

fn die(msg: &str) -> ! {
    eprintln!("sunbliss: {msg}");
    std::process::exit(2)
}

fn set(p: &mut Params, key: &str, v: &serde_json::Value) {
    let f = || v.as_f64().unwrap_or_else(|| die(&format!("{key}: expected number")));
    let b = || v.as_bool().or_else(|| v.as_f64().map(|x| x != 0.0)).unwrap_or_else(|| die(&format!("{key}: expected bool")));
    let s = || v.as_str().map(str::to_ascii_lowercase).unwrap_or_else(|| die(&format!("{key}: expected string")));
    match key {
        "input" => p.input_db = f(),
        "output" => p.output_db = f(),
        "link" => p.link = b(),
        "link_offset" => p.link_offset_db = f(),
        "ceiling" => p.ceiling_db = f(),
        "clip_on" => p.clip_on = b(),
        "clip_mode" => {
            p.clip_mode = match s().as_str() {
                "a" | "converter a" | "converter_a" => ClipMode::ConverterA,
                "b" | "converter b" | "converter_b" => ClipMode::ConverterB,
                "hard" => ClipMode::Hard,
                o => die(&format!("clip_mode: {o}")),
            }
        }
        "clip_filter" => p.clip_filter = b(),
        "guard" => p.guard = b(),
        "sat_on" => p.sat_on = b(),
        "sat" => p.sat = f(),
        "curve" => p.curve = if s() == "b" { Curve::B } else { Curve::A },
        "unity" => p.unity = b(),
        "contour_on" => p.contour_on = b(),
        "contour" => p.contour = f(),
        "tone" => {
            p.tone = match s().as_str() {
                "flat" => ToneMode::Flat,
                "a" => ToneMode::A,
                "b" => ToneMode::B,
                o => die(&format!("tone: {o}")),
            }
        }
        "wet" => p.wet = f() / 100.0,
        "dry" => p.dry = f() / 100.0,
        "mix_link" => p.mix_link = b(),
        "wet_mute" => p.wet_mute = b(),
        "dry_mute" => p.dry_mute = b(),
        "delta" => p.delta = b(),
        "bypass" => p.bypass = b(),
        "quality" => {
            p.quality = match s().as_str() {
                "high" => Quality::High,
                "pristine" => Quality::Pristine,
                "extra" => Quality::Extra,
                o => die(&format!("quality: {o}")),
            }
        }
        "neutral" => {
            if b() {
                *p = Params::neutral()
            }
        }
        o => die(&format!("unknown parameter {o}")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let mut kv: Vec<(String, serde_json::Value)> = Vec::new();
    let mut block = 512usize;
    let mut compensate = false;
    let mut rate = 48000.0;
    let mut channels = 2usize;
    let mut print_latency = false;
    let mut bench: Option<f64> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let mut next = || {
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| die(&format!("{a} needs a value")))
        };
        match a.as_str() {
            "--params" => {
                let txt = std::fs::read_to_string(next()).unwrap_or_else(|e| die(&e.to_string()));
                let m: BTreeMap<String, serde_json::Value> = serde_json::from_str(&txt).unwrap_or_else(|e| die(&e.to_string()));
                // "neutral" first so other keys override it
                if let Some(v) = m.get("neutral") {
                    kv.push(("neutral".into(), v.clone()));
                }
                kv.extend(m.into_iter().filter(|(k, _)| k != "neutral"));
            }
            "--set" => {
                let s = next();
                let (k, v) = s.split_once('=').unwrap_or_else(|| die("--set key=value"));
                let val = serde_json::from_str(v).unwrap_or(serde_json::Value::String(v.to_string()));
                kv.push((k.to_string(), val));
            }
            "--block" => block = next().parse().unwrap_or_else(|_| die("--block")),
            "--rate" => rate = next().parse().unwrap_or_else(|_| die("--rate")),
            "--channels" => channels = next().parse().unwrap_or_else(|_| die("--channels")),
            "--compensate" => compensate = true,
            "--latency" => print_latency = true,
            "--bench" => bench = Some(next().parse().unwrap_or_else(|_| die("--bench SECONDS"))),
            _ => pos.push(a.clone()),
        }
        i += 1;
    }
    let mut p = Params::default();
    for (k, v) in &kv {
        set(&mut p, k, v);
    }
    if print_latency && pos.is_empty() {
        let e = Engine::new(rate, channels, block);
        let mut e2 = e;
        e2.set_params(&p);
        e2.snap();
        println!("{}", e2.latency());
        return;
    }
    if let Some(secs) = bench {
        // CPU benchmark: process `secs` of stereo pink-ish noise in memory
        let mut e = Engine::new(rate, 2, block);
        e.set_params(&p);
        e.snap();
        let n = (secs * rate) as usize;
        let mut s: u64 = 1;
        let mut lp = 0.0;
        let src: Vec<f64> = (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                lp += 0.2 * (((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0 - lp);
                lp * 3.0
            })
            .collect();
        let mut a = vec![0.0; block];
        let mut b = vec![0.0; block];
        let t = std::time::Instant::now();
        let mut pos = 0;
        while pos < n {
            let m = block.min(n - pos);
            a[..m].copy_from_slice(&src[pos..pos + m]);
            b[..m].copy_from_slice(&src[pos..pos + m]);
            let mut refs: [&mut [f64]; 2] = [&mut a[..m], &mut b[..m]];
            e.process(&mut refs, m);
            pos += m;
        }
        let el = t.elapsed().as_secs_f64();
        println!("{:.4}", el / secs);
        return;
    }
    if pos.len() != 2 {
        die("usage: sunbliss-cli IN OUT [options]");
    }
    // ---- read input (deinterleaved f64)
    let (fs, data): (f64, Vec<Vec<f64>>) = if pos[0].ends_with(".f64") {
        let mut bytes = Vec::new();
        std::fs::File::open(&pos[0]).and_then(|mut f| f.read_to_end(&mut bytes)).unwrap_or_else(|e| die(&e.to_string()));
        let v: Vec<f64> = bytes.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect();
        let frames = v.len() / channels;
        (rate, (0..channels).map(|c| (0..frames).map(|f| v[f * channels + c]).collect()).collect())
    } else {
        let mut r = hound::WavReader::open(&pos[0]).unwrap_or_else(|e| die(&e.to_string()));
        let spec = r.spec();
        channels = spec.channels as usize;
        let inter: Vec<f64> = match spec.sample_format {
            hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap() as f64).collect(),
            hound::SampleFormat::Int => {
                let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f64;
                r.samples::<i32>().map(|s| s.unwrap() as f64 * scale).collect()
            }
        };
        let frames = inter.len() / channels;
        (spec.sample_rate as f64, (0..channels).map(|c| (0..frames).map(|f| inter[f * channels + c]).collect()).collect())
    };
    let frames = data[0].len();
    let mut e = Engine::new(fs, channels, block);
    e.set_params(&p);
    e.snap();
    e.reset();
    let lat = e.latency();
    eprintln!("sunbliss: {fs} Hz, {channels} ch, {frames} frames, oversampling {}x, latency {lat}", e.oversampling());
    let total = if compensate { frames + lat } else { frames };
    let mut out: Vec<Vec<f64>> = vec![Vec::with_capacity(total); channels];
    let mut bufs: Vec<Vec<f64>> = vec![vec![0.0; block]; channels];
    let mut pos_in = 0;
    while pos_in < total {
        let n = block.min(total - pos_in);
        for c in 0..channels {
            for k in 0..n {
                bufs[c][k] = data[c].get(pos_in + k).copied().unwrap_or(0.0);
            }
        }
        {
            let mut refs: Vec<&mut [f64]> = bufs.iter_mut().map(|b| &mut b[..n]).collect();
            e.process(&mut refs, n);
        }
        for c in 0..channels {
            out[c].extend_from_slice(&bufs[c][..n]);
        }
        pos_in += n;
    }
    if compensate {
        for c in out.iter_mut() {
            c.drain(..lat);
        }
    }
    // ---- write
    if pos[1].ends_with(".f64") {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&pos[1]).unwrap_or_else(|e| die(&e.to_string())));
        for k in 0..out[0].len() {
            for c in out.iter() {
                f.write_all(&c[k].to_le_bytes()).unwrap();
            }
        }
    } else {
        let spec = hound::WavSpec { channels: channels as u16, sample_rate: fs as u32, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(&pos[1], spec).unwrap_or_else(|e| die(&e.to_string()));
        for k in 0..out[0].len() {
            for c in out.iter() {
                w.write_sample(c[k] as f32).unwrap();
            }
        }
        w.finalize().unwrap();
    }
}
