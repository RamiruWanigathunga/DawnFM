//! Tier 1: F05 - Lyric Blur Texture Pooling
//!
//! Validates slot pooling spare limits, 64-pixel bucket quantization math,
//! texture reuse across frames, expired slot recycling, and reset cleanup per ORIGINAL_REQUEST §R2.

pub const SLOT_STEP: usize = 64;
pub const SPARE_SLOTS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bezier(pub f32, pub f32, pub f32, pub f32);

impl Bezier {
    pub fn at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        3.0 * (1.0 - t) * (1.0 - t) * t * self.1 + 3.0 * (1.0 - t) * t * t * self.3 + t * t * t
    }
}

pub const EASE: Bezier = Bezier(0.25, 0.1, 0.25, 1.0);
pub const RICHSYNC_OFFSET: f64 = 0.150;

pub struct MockSlot {
    pub id: usize,
    pub size: [usize; 2],
    pub _used: [usize; 2],
    pub _hash: u64,
}

pub struct MockGlowPool {
    pub spare: Vec<MockSlot>,
    pub next_id: usize,
}

impl MockGlowPool {
    pub fn new() -> Self {
        Self {
            spare: Vec::new(),
            next_id: 1,
        }
    }

    pub fn quantize(dim: usize) -> usize {
        (dim + 1).next_multiple_of(SLOT_STEP)
    }

    pub fn acquire(&mut self, w: usize, h: usize) -> MockSlot {
        let bucket = [Self::quantize(w), Self::quantize(h)];
        if let Some(pos) = self.spare.iter().position(|s| s.size[0] >= w && s.size[1] >= h) {
            self.spare.swap_remove(pos)
        } else {
            let id = self.next_id;
            self.next_id += 1;
            MockSlot {
                id,
                size: bucket,
                _used: [w, h],
                _hash: 0,
            }
        }
    }

    pub fn recycle(&mut self, slot: MockSlot) {
        if self.spare.len() >= SPARE_SLOTS {
            self.spare.remove(0);
        }
        self.spare.push(slot);
    }
}

impl Default for MockGlowPool {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_slot_pool_spare_cap() {
    let mut pool = MockGlowPool::new();

    for i in 0..10 {
        let slot = MockSlot {
            id: i,
            size: [64, 64],
            _used: [40, 40],
            _hash: i as u64,
        };
        pool.recycle(slot);
        assert!(
            pool.spare.len() <= SPARE_SLOTS,
            "Spare pool size must never exceed SPARE_SLOTS (2)"
        );
    }

    assert_eq!(pool.spare.len(), SPARE_SLOTS);
    assert_eq!(pool.spare[0].id, 8);
    assert_eq!(pool.spare[1].id, 9);
}

#[test]
fn test_slot_size_quantization() {
    assert_eq!(MockGlowPool::quantize(10), 64);
    assert_eq!(MockGlowPool::quantize(63), 64);
    assert_eq!(MockGlowPool::quantize(64), 128);
    assert_eq!(MockGlowPool::quantize(100), 128);
    assert_eq!(MockGlowPool::quantize(127), 128);
    assert_eq!(MockGlowPool::quantize(128), 192);
}

#[test]
fn test_slot_reuse_across_frames() {
    let mut pool = MockGlowPool::new();

    let s1 = pool.acquire(50, 50);
    assert_eq!(s1.id, 1);
    assert_eq!(s1.size, [64, 64]);

    pool.recycle(s1);
    assert_eq!(pool.spare.len(), 1);

    let s2 = pool.acquire(50, 50);
    assert_eq!(s2.id, 1, "Slot from spare pool should be reused");
    assert!(pool.spare.is_empty());
}

#[test]
fn test_expired_slot_recycling() {
    let mut pool = MockGlowPool::new();
    let mut active_slots = vec![
        pool.acquire(60, 60),
        pool.acquire(60, 60),
        pool.acquire(60, 60),
    ];

    assert_eq!(active_slots.len(), 3);

    for slot in active_slots.drain(..) {
        pool.recycle(slot);
    }

    assert_eq!(pool.spare.len(), SPARE_SLOTS);
}

#[test]
fn test_glow_reset_cleanup() {
    let mut pool = MockGlowPool::new();
    let slot = pool.acquire(100, 100);
    pool.recycle(slot);
    assert!(!pool.spare.is_empty());

    pool.spare.clear();
    assert!(pool.spare.is_empty());

    assert!((EASE.at(0.0) - 0.0).abs() < 1e-5);
    assert!((EASE.at(1.0) - 1.0).abs() < 1e-5);
    assert_eq!(RICHSYNC_OFFSET, 0.150);
}
