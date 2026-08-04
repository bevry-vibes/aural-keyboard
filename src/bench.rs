//! `bench`: measures trigger→voice-start latency (p50/p95/p99) — the part of the
//! pipeline this program controls. OS hook dispatch happens before our callback
//! and is not included (documented in DESIGN.md §5).

use anyhow::Result;
use std::time::{Duration, Instant};

use crate::engine::{self, Engine};
use crate::mapping;
use crate::mixer::Trigger;

pub fn report(samples_ns: &[u64]) -> String {
    if samples_ns.is_empty() {
        return "no samples collected".to_string();
    }
    let mut s = samples_ns.to_vec();
    s.sort_unstable();
    let q = |p: f64| s[((s.len() - 1) as f64 * p) as usize] as f64 / 1e6;
    format!(
        "samples={}  p50={:.2}ms  p95={:.2}ms  p99={:.2}ms  max={:.2}ms",
        s.len(),
        q(0.50),
        q(0.95),
        q(0.99),
        *s.last().unwrap() as f64 / 1e6
    )
}

/// Synthetic: fire `count` note-ons straight into the engine queue (no hook needed).
pub fn synthetic(count: usize) -> Result<()> {
    let (engine, device, supported) = Engine::new()?;
    let (bench_tx, bench_rx) = crossbeam_channel::unbounded();
    let mixer = engine.mixer(Some(bench_tx));
    let (_stream, info) = crate::audio::start_stream(&device, &supported, mixer)?;
    eprintln!(
        "aural bench: '{}' @ {} Hz, buffer {}",
        info.device_name, info.sample_rate, info.buffer_desc
    );

    for (i, vk) in (mapping::VK_A..=mapping::VK_Z)
        .cycle()
        .take(count)
        .enumerate()
    {
        let n = mapping::map_key(vk, i % 2 == 0).expect("letters always map");
        engine.trigger_tx.send(Trigger::NoteOn {
            key: vk,
            instrument: n.instrument,
            midi: n.midi,
            velocity: n.velocity,
            at: Instant::now(),
        })?;
        // Detach immediately so retrigger dedup never drops a sample.
        engine.trigger_tx.send(Trigger::NoteOff { key: vk })?;
        std::thread::sleep(Duration::from_millis(3));
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut samples = Vec::with_capacity(count);
    while samples.len() < count && Instant::now() < deadline {
        match bench_rx.recv_deadline(deadline) {
            Ok(v) => samples.push(v),
            Err(_) => break,
        }
    }
    println!("{}", report(&samples));
    Ok(())
}

/// Live: full engine + hook; type freely, Ctrl+C prints the report.
pub fn live() -> Result<()> {
    let (bench_tx, bench_rx) = crossbeam_channel::unbounded();
    engine::run(false, Some(bench_tx))?;
    let samples: Vec<u64> = bench_rx.try_iter().collect();
    println!("{}", report(&samples));
    Ok(())
}
