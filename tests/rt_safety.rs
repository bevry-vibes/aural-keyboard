//! RT-safety proof: the render path must never allocate (DESIGN.md §5, D6).
//! Runs in its own test binary so the counting allocator sees only this test.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

static ARMED: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicU64 = AtomicU64::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn render_path_does_not_allocate() {
    let rate = 48_000;
    let (tx, rx) = crossbeam_channel::unbounded();
    let flags = Arc::new(aural::mixer::SharedFlags::new(1.0, false));
    let mut mixer = aural::mixer::AuralMixer::new(
        rx,
        aural::assets::SampleBank::synthetic(rate),
        flags,
        rate,
        2,
        None,
    );

    let n = aural::mapping::map_key(aural::mapping::VK_A, false).unwrap();
    tx.send(aural::mixer::Trigger::NoteOn {
        key: aural::mapping::VK_A,
        instrument: n.instrument,
        midi: n.midi,
        velocity: n.velocity,
        at: Instant::now(),
    })
    .unwrap();

    let mut buf = vec![0.0f32; 128 * 2];
    mixer.fill(&mut buf); // warm-up: drains the trigger, starts the voice
    assert!(
        buf.iter().any(|s| s.abs() > 1e-6),
        "setup sanity: voice audible"
    );

    ARMED.store(true, Ordering::Relaxed);
    COUNT.store(0, Ordering::Relaxed);
    for _ in 0..1000 {
        mixer.fill(&mut buf);
    }
    ARMED.store(false, Ordering::Relaxed);

    assert_eq!(
        COUNT.load(Ordering::Relaxed),
        0,
        "render path allocated during fill()"
    );
}
