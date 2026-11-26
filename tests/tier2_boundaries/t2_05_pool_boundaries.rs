//! Tier 2: F05 - Lyric Blur Texture Pooling Boundaries
//!
//! Validates blur radius zero bypass, extreme blur radius clamping, 10,000-cycle slot pool pressure,
//! atlas resize invalidation, and negative/NaN radius rejection per ORIGINAL_REQUEST §R2.

pub const SLOT_STEP: usize = 64;
pub const SPARE_SLOTS: usize = 2;
pub const MAX_BLUR_RADIUS: f32 = 64.0;

pub fn clamp_blur_radius(r: f32) -> f32 {
    if r.is_nan() || r <= 0.0 {
        0.0
    } else {
        r.min(MAX_BLUR_RADIUS)
    }
}

pub struct SlotPool {
    pub spare: Vec<[usize; 2]>,
    pub allocated_count: usize,
}

impl SlotPool {
    pub fn new() -> Self {
        Self {
            spare: Vec::new(),
            allocated_count: 0,
        }
    }

    pub fn quantize(dim: usize) -> usize {
        (dim + 1).next_multiple_of(SLOT_STEP)
    }

    pub fn acquire(&mut self, w: usize, h: usize) -> [usize; 2] {
        let qw = Self::quantize(w);
        let qh = Self::quantize(h);

        if let Some(pos) = self.spare.iter().position(|&s| s[0] >= qw && s[1] >= qh) {
            self.spare.swap_remove(pos)
        } else {
            self.allocated_count += 1;
            [qw, qh]
        }
    }

    pub fn release(&mut self, slot: [usize; 2]) {
        if self.spare.len() < SPARE_SLOTS {
            self.spare.push(slot);
        }
    }
}

#[test]
fn test_blur_radius_zero_bypass() {
    let r_zero = 0.0f32;
    let clamped = clamp_blur_radius(r_zero);
    assert_eq!(clamped, 0.0);

    // If blur radius is 0, no blur pass is required (bypass)
    let requires_blur = clamped > 0.0;
    assert!(!requires_blur, "0.0 blur radius must bypass blur convolution pass");
}

#[test]
fn test_blur_radius_extreme_clamp() {
    let extreme_radii = [100.0, 500.0, 1000.0, 50_000.0, f32::MAX];

    for &r in &extreme_radii {
        let clamped = clamp_blur_radius(r);
        assert!(clamped.is_finite());
        assert_eq!(
            clamped, MAX_BLUR_RADIUS,
            "Extreme blur radius {} must clamp strictly to MAX_BLUR_RADIUS ({})",
            r, MAX_BLUR_RADIUS
        );
    }
}

#[test]
fn test_pool_10k_rapid_slot_pressure() {
    let mut pool = SlotPool::new();

    for i in 0..10_000 {
        let w = 20 + (i * 17) % 400;
        let h = 20 + (i * 31) % 100;
        let slot = pool.acquire(w, h);

        assert_eq!(slot[0] % SLOT_STEP, 0, "Width must be multiple of 64");
        assert_eq!(slot[1] % SLOT_STEP, 0, "Height must be multiple of 64");
        assert!(slot[0] >= w);
        assert!(slot[1] >= h);

        pool.release(slot);
        assert!(
            pool.spare.len() <= SPARE_SLOTS,
            "Spare pool count must never exceed SPARE_SLOTS"
        );
    }
}

#[test]
fn test_pool_atlas_resize_invalidation() {
    let mut pool = SlotPool::new();

    // 1. Initial standard size acquisition (100x100)
    let s1 = pool.acquire(100, 100);
    assert_eq!(s1, [128, 128]);
    pool.release(s1);
    assert_eq!(pool.spare.len(), 1);

    // 2. Large 4K resize acquisition (3840x2160)
    let s2 = pool.acquire(3840, 2160);
    // 128x128 spare cannot satisfy 3840x2160, so a new slot is allocated
    assert_eq!(s2[0], SlotPool::quantize(3840));
    assert_eq!(s2[1], SlotPool::quantize(2160));
    assert_eq!(pool.allocated_count, 2);

    pool.release(s2);
    assert_eq!(pool.spare.len(), 2);
}

#[test]
fn test_pool_negative_radius_rejection() {
    let invalid_radii = [
        -0.001f32,
        -1.0,
        -100.0,
        f32::NEG_INFINITY,
        f32::NAN,
    ];

    for &r in &invalid_radii {
        let clamped = clamp_blur_radius(r);
        assert!(!clamped.is_nan(), "Clamped blur radius must not be NaN");
        assert_eq!(
            clamped, 0.0,
            "Negative, -Inf, or NaN blur radius must clamp strictly to 0.0"
        );
    }
}
