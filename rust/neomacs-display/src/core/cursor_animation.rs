//! Cursor animation system - Neovide-style smooth cursor with particle effects.

use std::time::{Duration, Instant};
use std::collections::VecDeque;

/// Cursor animation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorAnimationMode {
    /// No animation - instant cursor movement
    None,
    /// Smooth movement only
    #[default]
    Smooth,
    /// Particles shoot backward (Neovide railgun)
    Railgun,
    /// Comet-like trail follows cursor (Neovide torpedo)
    Torpedo,
    /// Sparkly particles scatter around (Neovide pixiedust)
    Pixiedust,
    /// Shockwave ring expands from cursor (Neovide sonicboom)
    Sonicboom,
    /// Concentric rings emanate outward (Neovide ripple)
    Ripple,
    /// Animated outline glow
    Wireframe,
}

impl CursorAnimationMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => Self::None,
            "smooth" => Self::Smooth,
            "railgun" => Self::Railgun,
            "torpedo" => Self::Torpedo,
            "pixiedust" => Self::Pixiedust,
            "sonicboom" => Self::Sonicboom,
            "ripple" => Self::Ripple,
            "wireframe" => Self::Wireframe,
            _ => Self::Smooth,
        }
    }
}

/// A single particle in the cursor trail
#[derive(Debug, Clone)]
pub struct Particle {
    /// Current X position
    pub x: f32,
    /// Current Y position  
    pub y: f32,
    /// X velocity (pixels per second)
    pub vx: f32,
    /// Y velocity (pixels per second)
    pub vy: f32,
    /// Current size (radius)
    pub size: f32,
    /// Color (RGBA)
    pub color: [f32; 4],
    /// Time when particle was created
    pub birth_time: Instant,
    /// Particle lifetime
    pub lifetime: Duration,
    /// Initial size (for decay calculation)
    pub initial_size: f32,
}

impl Particle {
    /// Check if particle is still alive
    pub fn is_alive(&self, now: Instant) -> bool {
        now.duration_since(self.birth_time) < self.lifetime
    }
    
    /// Get current age as fraction (0.0 = just born, 1.0 = dead)
    pub fn age_fraction(&self, now: Instant) -> f32 {
        let age = now.duration_since(self.birth_time).as_secs_f32();
        let lifetime = self.lifetime.as_secs_f32();
        (age / lifetime).min(1.0)
    }
    
    /// Update particle position based on velocity
    pub fn update(&mut self, dt: f32) {
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        // Apply friction/drag
        self.vx *= 0.95;
        self.vy *= 0.95;
    }
    
    /// Get current opacity (fades out over lifetime)
    pub fn opacity(&self, now: Instant) -> f32 {
        let age = self.age_fraction(now);
        // Smooth fade out
        (1.0 - age).powi(2)
    }
    
    /// Get current size (shrinks over lifetime)
    pub fn current_size(&self, now: Instant) -> f32 {
        let age = self.age_fraction(now);
        self.initial_size * (1.0 - age * 0.7)
    }
}

/// Ring effect (for sonicboom/ripple)
#[derive(Debug, Clone)]
pub struct Ring {
    /// Center X
    pub x: f32,
    /// Center Y
    pub y: f32,
    /// Current radius
    pub radius: f32,
    /// Expansion speed (pixels per second)
    pub speed: f32,
    /// Color
    pub color: [f32; 4],
    /// Birth time
    pub birth_time: Instant,
    /// Lifetime
    pub lifetime: Duration,
    /// Ring thickness
    pub thickness: f32,
}

impl Ring {
    pub fn is_alive(&self, now: Instant) -> bool {
        now.duration_since(self.birth_time) < self.lifetime
    }
    
    pub fn age_fraction(&self, now: Instant) -> f32 {
        let age = now.duration_since(self.birth_time).as_secs_f32();
        (age / self.lifetime.as_secs_f32()).min(1.0)
    }
    
    pub fn update(&mut self, dt: f32) {
        self.radius += self.speed * dt;
    }
    
    pub fn opacity(&self, now: Instant) -> f32 {
        let age = self.age_fraction(now);
        (1.0 - age).powi(2)
    }
}

/// Trail point for torpedo effect
#[derive(Debug, Clone)]
pub struct TrailPoint {
    pub x: f32,
    pub y: f32,
    pub time: Instant,
}

/// Cursor animation state
#[derive(Debug)]
pub struct CursorAnimator {
    /// Animation mode
    pub mode: CursorAnimationMode,
    
    /// Target cursor position (from Emacs)
    pub target_x: f32,
    pub target_y: f32,
    pub target_width: f32,
    pub target_height: f32,
    
    /// Current animated cursor position
    pub current_x: f32,
    pub current_y: f32,
    pub current_width: f32,
    pub current_height: f32,
    
    /// Cursor color
    pub color: [f32; 4],
    
    /// Cursor style (0=box, 1=bar, 2=underline, 3=hollow)
    pub style: u8,
    
    /// Is cursor visible (for blink)
    pub visible: bool,
    
    /// Blink state
    blink_on: bool,
    last_blink_toggle: Instant,
    blink_interval: Duration,
    
    /// Animation speed (higher = faster)
    pub animation_speed: f32,
    
    /// Particle system
    pub particles: Vec<Particle>,
    
    /// Ring effects
    pub rings: Vec<Ring>,
    
    /// Trail points for torpedo
    pub trail: VecDeque<TrailPoint>,
    max_trail_length: usize,
    
    /// Last update time
    last_update: Instant,
    
    /// Last position (for detecting movement)
    last_target_x: f32,
    last_target_y: f32,
    
    /// Particle settings
    particle_count: u32,
    particle_lifetime: Duration,
    particle_speed: f32,
    particle_size: f32,
    
    /// Glow intensity (0.0 - 1.0)
    pub glow_intensity: f32,
    
    /// Whether animation is active (cursor is moving)
    animating: bool,
}

impl Default for CursorAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorAnimator {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            mode: CursorAnimationMode::Smooth,
            target_x: 0.0,
            target_y: 0.0,
            target_width: 8.0,
            target_height: 16.0,
            current_x: 0.0,
            current_y: 0.0,
            current_width: 8.0,
            current_height: 16.0,
            color: [1.0, 1.0, 1.0, 1.0],
            style: 0,
            visible: true,
            blink_on: true,
            last_blink_toggle: now,
            blink_interval: Duration::from_millis(530),
            animation_speed: 15.0, // Neovide default-ish
            particles: Vec::with_capacity(100),
            rings: Vec::with_capacity(10),
            trail: VecDeque::with_capacity(50),
            max_trail_length: 40,
            last_update: now,
            last_target_x: 0.0,
            last_target_y: 0.0,
            particle_count: 15,
            particle_lifetime: Duration::from_millis(400),
            particle_speed: 200.0,
            particle_size: 4.0,
            glow_intensity: 0.3,
            animating: false,
        }
    }
    
    /// Set cursor target position (called when Emacs updates cursor)
    pub fn set_target(&mut self, x: f32, y: f32, width: f32, height: f32, style: u8, color: [f32; 4]) {
        let moved = (self.target_x - x).abs() > 0.5 || (self.target_y - y).abs() > 0.5;
        
        self.last_target_x = self.target_x;
        self.last_target_y = self.target_y;
        self.target_x = x;
        self.target_y = y;
        self.target_width = width;
        self.target_height = height;
        self.style = style;
        self.color = color;
        
        if moved {
            self.on_cursor_move();
        }
    }
    
    /// Called when cursor moves - spawn effects
    fn on_cursor_move(&mut self) {
        self.animating = true;
        
        // Reset blink when cursor moves
        self.blink_on = true;
        self.last_blink_toggle = Instant::now();
        
        let now = Instant::now();
        let dx = self.target_x - self.last_target_x;
        let dy = self.target_y - self.last_target_y;
        let distance = (dx * dx + dy * dy).sqrt();
        
        if distance < 1.0 {
            return;
        }
        
        // Spawn effects based on mode
        match self.mode {
            CursorAnimationMode::None | CursorAnimationMode::Smooth => {}
            
            CursorAnimationMode::Railgun => {
                self.spawn_railgun_particles(dx, dy, distance);
            }
            
            CursorAnimationMode::Torpedo => {
                self.add_trail_point();
            }
            
            CursorAnimationMode::Pixiedust => {
                self.spawn_pixiedust_particles();
            }
            
            CursorAnimationMode::Sonicboom => {
                self.spawn_sonicboom();
            }
            
            CursorAnimationMode::Ripple => {
                self.spawn_ripple();
            }
            
            CursorAnimationMode::Wireframe => {
                // Wireframe is rendered differently, no particles
            }
        }
    }
    
    fn spawn_railgun_particles(&mut self, dx: f32, dy: f32, distance: f32) {
        let now = Instant::now();
        let norm_dx = -dx / distance; // Opposite direction
        let norm_dy = -dy / distance;
        
        // Spawn particles at current position shooting backward
        for i in 0..self.particle_count {
            let angle_offset = (i as f32 / self.particle_count as f32 - 0.5) * 0.8;
            let cos_a = angle_offset.cos();
            let sin_a = angle_offset.sin();
            
            // Rotate direction by angle offset
            let vx = (norm_dx * cos_a - norm_dy * sin_a) * self.particle_speed;
            let vy = (norm_dx * sin_a + norm_dy * cos_a) * self.particle_speed;
            
            // Add some randomness
            let rand_factor = 0.5 + (i as f32 * 7.13).sin().abs() * 0.5;
            
            self.particles.push(Particle {
                x: self.current_x + self.current_width / 2.0,
                y: self.current_y + self.current_height / 2.0,
                vx: vx * rand_factor,
                vy: vy * rand_factor,
                size: self.particle_size * rand_factor,
                color: self.color,
                birth_time: now,
                lifetime: Duration::from_millis((self.particle_lifetime.as_millis() as f32 * rand_factor) as u64),
                initial_size: self.particle_size * rand_factor,
            });
        }
    }
    
    fn spawn_pixiedust_particles(&mut self) {
        let now = Instant::now();
        
        for i in 0..self.particle_count {
            // Random direction
            let angle = (i as f32 * 2.39996) % (2.0 * std::f32::consts::PI); // Golden angle
            let speed = self.particle_speed * (0.3 + (i as f32 * std::f32::consts::PI).sin().abs() * 0.7);
            
            self.particles.push(Particle {
                x: self.current_x + self.current_width / 2.0,
                y: self.current_y + self.current_height / 2.0,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                size: self.particle_size * 0.7,
                color: [
                    self.color[0],
                    self.color[1], 
                    self.color[2],
                    self.color[3] * 0.8,
                ],
                birth_time: now,
                lifetime: self.particle_lifetime,
                initial_size: self.particle_size * 0.7,
            });
        }
    }
    
    fn add_trail_point(&mut self) {
        self.trail.push_back(TrailPoint {
            x: self.current_x + self.current_width / 2.0,
            y: self.current_y + self.current_height / 2.0,
            time: Instant::now(),
        });
        
        while self.trail.len() > self.max_trail_length {
            self.trail.pop_front();
        }
    }
    
    fn spawn_sonicboom(&mut self) {
        let now = Instant::now();
        self.rings.push(Ring {
            x: self.target_x + self.target_width / 2.0,
            y: self.target_y + self.target_height / 2.0,
            radius: 5.0,
            speed: 300.0,
            color: self.color,
            birth_time: now,
            lifetime: Duration::from_millis(300),
            thickness: 3.0,
        });
    }
    
    fn spawn_ripple(&mut self) {
        let now = Instant::now();
        // Spawn multiple concentric rings
        for i in 0..3 {
            self.rings.push(Ring {
                x: self.target_x + self.target_width / 2.0,
                y: self.target_y + self.target_height / 2.0,
                radius: 2.0 + i as f32 * 8.0,
                speed: 150.0 - i as f32 * 20.0,
                color: self.color,
                birth_time: now,
                lifetime: Duration::from_millis(400 + i as u64 * 50),
                thickness: 2.0,
            });
        }
    }
    
    /// Update animation state - call each frame
    /// Returns true if animation is still active (needs redraw)
    pub fn update(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;
        
        // Update cursor blink
        if now.duration_since(self.last_blink_toggle) >= self.blink_interval {
            self.blink_on = !self.blink_on;
            self.last_blink_toggle = now;
        }
        
        // Smooth cursor movement (exponential interpolation)
        if self.mode != CursorAnimationMode::None {
            let factor = 1.0 - (-self.animation_speed * dt).exp();
            
            self.current_x += (self.target_x - self.current_x) * factor;
            self.current_y += (self.target_y - self.current_y) * factor;
            self.current_width += (self.target_width - self.current_width) * factor;
            self.current_height += (self.target_height - self.current_height) * factor;
            
            // Check if we've reached the target
            let dx = (self.target_x - self.current_x).abs();
            let dy = (self.target_y - self.current_y).abs();
            if dx < 0.5 && dy < 0.5 {
                self.current_x = self.target_x;
                self.current_y = self.target_y;
                self.animating = false;
            }
        } else {
            // No animation - instant movement
            self.current_x = self.target_x;
            self.current_y = self.target_y;
            self.current_width = self.target_width;
            self.current_height = self.target_height;
            self.animating = false;
        }
        
        // Update particles
        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| p.is_alive(now));
        
        // Update rings
        for ring in &mut self.rings {
            ring.update(dt);
        }
        self.rings.retain(|r| r.is_alive(now));
        
        // Update trail (remove old points)
        let trail_lifetime = Duration::from_millis(200);
        self.trail.retain(|p| now.duration_since(p.time) < trail_lifetime);
        
        // Add trail point for torpedo while moving
        if self.mode == CursorAnimationMode::Torpedo && self.animating {
            self.add_trail_point();
        }
        
        // Return true if any animation is active
        self.animating || !self.particles.is_empty() || !self.rings.is_empty() || !self.trail.is_empty()
    }
    
    /// Get cursor visibility (considering blink)
    pub fn is_visible(&self) -> bool {
        self.visible && self.blink_on
    }
    
    /// Check if cursor is currently animating
    pub fn is_animating(&self) -> bool {
        self.animating || !self.particles.is_empty() || !self.rings.is_empty()
    }
    
    /// Set animation mode
    pub fn set_mode(&mut self, mode: CursorAnimationMode) {
        self.mode = mode;
        // Clear effects when changing mode
        self.particles.clear();
        self.rings.clear();
        self.trail.clear();
    }
    
    /// Set animation speed (higher = faster cursor movement)
    pub fn set_animation_speed(&mut self, speed: f32) {
        self.animation_speed = speed.max(1.0).min(100.0);
    }
    
    /// Set particle count for effects
    pub fn set_particle_count(&mut self, count: u32) {
        self.particle_count = count.max(1).min(100);
    }

    /// Update with explicit delta time (for external time management)
    pub fn update_with_dt(&mut self, dt: f32) -> bool {
        let now = Instant::now();

        // Update cursor blink
        if now.duration_since(self.last_blink_toggle) >= self.blink_interval {
            self.blink_on = !self.blink_on;
            self.last_blink_toggle = now;
        }

        // Smooth cursor movement (exponential interpolation)
        if self.mode != CursorAnimationMode::None {
            let factor = 1.0 - (-self.animation_speed * dt).exp();

            self.current_x += (self.target_x - self.current_x) * factor;
            self.current_y += (self.target_y - self.current_y) * factor;
            self.current_width += (self.target_width - self.current_width) * factor;
            self.current_height += (self.target_height - self.current_height) * factor;

            // Check if we've reached the target
            let dx = (self.target_x - self.current_x).abs();
            let dy = (self.target_y - self.current_y).abs();
            if dx < 0.5 && dy < 0.5 {
                self.current_x = self.target_x;
                self.current_y = self.target_y;
                self.animating = false;
            }
        } else {
            // No animation - instant movement
            self.current_x = self.target_x;
            self.current_y = self.target_y;
            self.current_width = self.target_width;
            self.current_height = self.target_height;
            self.animating = false;
        }

        // Update particles
        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| p.is_alive(now));

        // Update rings
        for ring in &mut self.rings {
            ring.update(dt);
        }
        self.rings.retain(|r| r.is_alive(now));

        // Update trail (remove old points)
        let trail_lifetime = Duration::from_millis(200);
        self.trail.retain(|p| now.duration_since(p.time) < trail_lifetime);

        // Add trail point for torpedo while moving
        if self.mode == CursorAnimationMode::Torpedo && self.animating {
            self.add_trail_point();
        }

        // Return true if any animation is active
        self.animating || !self.particles.is_empty() || !self.rings.is_empty() || !self.trail.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{Duration, Instant};

    // -----------------------------------------------------------------------
    // CursorAnimationMode
    // -----------------------------------------------------------------------

    #[test]
    fn mode_from_str_known_variants() {
        assert_eq!(CursorAnimationMode::from_str("none"), CursorAnimationMode::None);
        assert_eq!(CursorAnimationMode::from_str("smooth"), CursorAnimationMode::Smooth);
        assert_eq!(CursorAnimationMode::from_str("railgun"), CursorAnimationMode::Railgun);
        assert_eq!(CursorAnimationMode::from_str("torpedo"), CursorAnimationMode::Torpedo);
        assert_eq!(CursorAnimationMode::from_str("pixiedust"), CursorAnimationMode::Pixiedust);
        assert_eq!(CursorAnimationMode::from_str("sonicboom"), CursorAnimationMode::Sonicboom);
        assert_eq!(CursorAnimationMode::from_str("ripple"), CursorAnimationMode::Ripple);
        assert_eq!(CursorAnimationMode::from_str("wireframe"), CursorAnimationMode::Wireframe);
    }

    #[test]
    fn mode_from_str_case_insensitive() {
        assert_eq!(CursorAnimationMode::from_str("RAILGUN"), CursorAnimationMode::Railgun);
        assert_eq!(CursorAnimationMode::from_str("Torpedo"), CursorAnimationMode::Torpedo);
        assert_eq!(CursorAnimationMode::from_str("NONE"), CursorAnimationMode::None);
    }

    #[test]
    fn mode_from_str_unknown_falls_back_to_smooth() {
        assert_eq!(CursorAnimationMode::from_str("unknown"), CursorAnimationMode::Smooth);
        assert_eq!(CursorAnimationMode::from_str(""), CursorAnimationMode::Smooth);
        assert_eq!(CursorAnimationMode::from_str("foobar"), CursorAnimationMode::Smooth);
    }

    #[test]
    fn mode_default_is_smooth() {
        assert_eq!(CursorAnimationMode::default(), CursorAnimationMode::Smooth);
    }

    // -----------------------------------------------------------------------
    // Particle
    // -----------------------------------------------------------------------

    fn make_particle(lifetime_ms: u64) -> Particle {
        Particle {
            x: 10.0,
            y: 20.0,
            vx: 100.0,
            vy: -50.0,
            size: 4.0,
            color: [1.0, 1.0, 1.0, 1.0],
            birth_time: Instant::now(),
            lifetime: Duration::from_millis(lifetime_ms),
            initial_size: 4.0,
        }
    }

    #[test]
    fn particle_is_alive_before_lifetime() {
        let p = make_particle(500);
        assert!(p.is_alive(Instant::now()));
    }

    #[test]
    fn particle_is_dead_after_lifetime() {
        let p = make_particle(10);
        thread::sleep(Duration::from_millis(15));
        assert!(!p.is_alive(Instant::now()));
    }

    #[test]
    fn particle_age_fraction_starts_near_zero() {
        let p = make_particle(1000);
        let age = p.age_fraction(Instant::now());
        assert!(age < 0.1, "expected age near 0, got {}", age);
    }

    #[test]
    fn particle_age_fraction_clamped_to_one() {
        let p = make_particle(10);
        thread::sleep(Duration::from_millis(20));
        let age = p.age_fraction(Instant::now());
        assert!((age - 1.0).abs() < f32::EPSILON, "expected age clamped to 1.0, got {}", age);
    }

    #[test]
    fn particle_update_moves_position() {
        let mut p = make_particle(500);
        let old_x = p.x;
        let old_y = p.y;
        p.update(0.016); // ~60 FPS frame
        assert!(p.x > old_x, "x should increase with positive vx");
        assert!(p.y < old_y, "y should decrease with negative vy");
    }

    #[test]
    fn particle_update_applies_drag() {
        let mut p = make_particle(500);
        let old_vx = p.vx;
        p.update(0.016);
        assert!(p.vx.abs() < old_vx.abs(), "velocity should decrease due to drag");
    }

    #[test]
    fn particle_opacity_starts_at_one() {
        let p = make_particle(1000);
        let op = p.opacity(Instant::now());
        assert!(op > 0.9, "opacity should be near 1.0 at birth, got {}", op);
    }

    #[test]
    fn particle_opacity_approaches_zero() {
        let p = make_particle(10);
        thread::sleep(Duration::from_millis(15));
        let op = p.opacity(Instant::now());
        assert!(op < 0.05, "opacity should be near 0 after lifetime, got {}", op);
    }

    #[test]
    fn particle_current_size_shrinks_over_time() {
        let p = make_particle(10);
        let initial = p.current_size(p.birth_time);
        thread::sleep(Duration::from_millis(15));
        let final_size = p.current_size(Instant::now());
        assert!(final_size < initial, "size should shrink; initial={}, final={}", initial, final_size);
    }

    // -----------------------------------------------------------------------
    // Ring
    // -----------------------------------------------------------------------

    fn make_ring(lifetime_ms: u64) -> Ring {
        Ring {
            x: 50.0,
            y: 60.0,
            radius: 5.0,
            speed: 300.0,
            color: [1.0, 0.0, 0.0, 1.0],
            birth_time: Instant::now(),
            lifetime: Duration::from_millis(lifetime_ms),
            thickness: 3.0,
        }
    }

    #[test]
    fn ring_is_alive_and_dies() {
        let r = make_ring(10);
        assert!(r.is_alive(Instant::now()));
        thread::sleep(Duration::from_millis(15));
        assert!(!r.is_alive(Instant::now()));
    }

    #[test]
    fn ring_update_expands_radius() {
        let mut r = make_ring(500);
        let old_radius = r.radius;
        r.update(0.016);
        assert!(r.radius > old_radius, "ring should expand");
    }

    #[test]
    fn ring_opacity_fades() {
        let r = make_ring(10);
        let op_start = r.opacity(r.birth_time);
        assert!((op_start - 1.0).abs() < f32::EPSILON);
        thread::sleep(Duration::from_millis(15));
        let op_end = r.opacity(Instant::now());
        assert!(op_end < 0.05, "ring opacity should fade near zero, got {}", op_end);
    }

    // -----------------------------------------------------------------------
    // CursorAnimator - construction and initial state
    // -----------------------------------------------------------------------

    #[test]
    fn new_animator_initial_state() {
        let a = CursorAnimator::new();
        assert_eq!(a.mode, CursorAnimationMode::Smooth);
        assert_eq!(a.target_x, 0.0);
        assert_eq!(a.target_y, 0.0);
        assert_eq!(a.current_x, 0.0);
        assert_eq!(a.current_y, 0.0);
        assert_eq!(a.target_width, 8.0);
        assert_eq!(a.target_height, 16.0);
        assert_eq!(a.current_width, 8.0);
        assert_eq!(a.current_height, 16.0);
        assert_eq!(a.color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(a.style, 0);
        assert!(a.visible);
        assert!(!a.animating);
        assert!(a.particles.is_empty());
        assert!(a.rings.is_empty());
        assert!(a.trail.is_empty());
        assert!(a.is_visible());
        assert!(!a.is_animating());
    }

    #[test]
    fn default_matches_new() {
        let a = CursorAnimator::default();
        let b = CursorAnimator::new();
        assert_eq!(a.mode, b.mode);
        assert_eq!(a.target_x, b.target_x);
        assert_eq!(a.animation_speed, b.animation_speed);
        assert_eq!(a.glow_intensity, b.glow_intensity);
    }

    // -----------------------------------------------------------------------
    // set_target / update_target
    // -----------------------------------------------------------------------

    #[test]
    fn set_target_updates_target_fields() {
        let mut a = CursorAnimator::new();
        a.set_target(100.0, 200.0, 10.0, 20.0, 1, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(a.target_x, 100.0);
        assert_eq!(a.target_y, 200.0);
        assert_eq!(a.target_width, 10.0);
        assert_eq!(a.target_height, 20.0);
        assert_eq!(a.style, 1);
        assert_eq!(a.color, [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn set_target_starts_animation_on_move() {
        let mut a = CursorAnimator::new();
        assert!(!a.animating);
        a.set_target(100.0, 200.0, 10.0, 20.0, 0, [1.0; 4]);
        assert!(a.animating, "moving cursor should set animating=true");
    }

    #[test]
    fn set_target_no_animation_for_tiny_move() {
        let mut a = CursorAnimator::new();
        // Move less than 0.5 in both axes -- should not trigger on_cursor_move
        a.set_target(0.3, 0.3, 8.0, 16.0, 0, [1.0; 4]);
        assert!(!a.animating, "sub-threshold move should not animate");
    }

    #[test]
    fn set_target_records_last_position() {
        let mut a = CursorAnimator::new();
        a.set_target(50.0, 60.0, 8.0, 16.0, 0, [1.0; 4]);
        assert_eq!(a.last_target_x, 0.0);
        assert_eq!(a.last_target_y, 0.0);
        a.set_target(100.0, 120.0, 8.0, 16.0, 0, [1.0; 4]);
        assert_eq!(a.last_target_x, 50.0);
        assert_eq!(a.last_target_y, 60.0);
    }

    // -----------------------------------------------------------------------
    // tick / update_with_dt - animation progress
    // -----------------------------------------------------------------------

    #[test]
    fn update_with_dt_moves_toward_target() {
        let mut a = CursorAnimator::new();
        a.set_target(200.0, 300.0, 8.0, 16.0, 0, [1.0; 4]);

        // Simulate several frames
        for _ in 0..10 {
            a.update_with_dt(0.016);
        }

        // Should have moved toward target but not necessarily reached it
        assert!(a.current_x > 0.0, "current_x should move toward target");
        assert!(a.current_y > 0.0, "current_y should move toward target");
        assert!(a.current_x <= 200.0, "current_x should not overshoot");
        assert!(a.current_y <= 300.0, "current_y should not overshoot");
    }

    #[test]
    fn update_with_dt_converges_to_target() {
        let mut a = CursorAnimator::new();
        a.set_target(100.0, 100.0, 8.0, 16.0, 0, [1.0; 4]);

        // Simulate many frames (~2 seconds at 60fps)
        for _ in 0..120 {
            a.update_with_dt(0.016);
        }

        assert!((a.current_x - 100.0).abs() < 0.5, "x should converge; got {}", a.current_x);
        assert!((a.current_y - 100.0).abs() < 0.5, "y should converge; got {}", a.current_y);
        // Once converged, snaps exactly and animating is false
        assert_eq!(a.current_x, a.target_x);
        assert_eq!(a.current_y, a.target_y);
        assert!(!a.animating);
    }

    #[test]
    fn update_with_dt_snaps_when_close() {
        let mut a = CursorAnimator::new();
        // Place current very close to target
        a.target_x = 10.0;
        a.target_y = 10.0;
        a.current_x = 9.8;
        a.current_y = 9.8;
        a.animating = true;

        a.update_with_dt(0.016);

        assert_eq!(a.current_x, 10.0, "should snap to target x");
        assert_eq!(a.current_y, 10.0, "should snap to target y");
        assert!(!a.animating);
    }

    #[test]
    fn update_returns_false_when_idle() {
        let mut a = CursorAnimator::new();
        // No target change, no particles -- should be idle
        let active = a.update_with_dt(0.016);
        assert!(!active, "update should return false when nothing is happening");
    }

    #[test]
    fn update_returns_true_while_animating() {
        let mut a = CursorAnimator::new();
        a.set_target(500.0, 500.0, 8.0, 16.0, 0, [1.0; 4]);
        let active = a.update_with_dt(0.016);
        assert!(active, "update should return true while cursor is in motion");
    }

    // -----------------------------------------------------------------------
    // Mode::None - instant movement
    // -----------------------------------------------------------------------

    #[test]
    fn mode_none_instant_movement() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::None);
        a.set_target(500.0, 400.0, 12.0, 24.0, 0, [1.0; 4]);
        a.update_with_dt(0.016);

        assert_eq!(a.current_x, 500.0);
        assert_eq!(a.current_y, 400.0);
        assert_eq!(a.current_width, 12.0);
        assert_eq!(a.current_height, 24.0);
        assert!(!a.animating);
    }

    // -----------------------------------------------------------------------
    // Blink
    // -----------------------------------------------------------------------

    #[test]
    fn blink_toggles_after_interval() {
        let mut a = CursorAnimator::new();
        assert!(a.blink_on);
        assert!(a.is_visible());

        // Wait longer than blink interval (530ms)
        thread::sleep(Duration::from_millis(550));
        a.update_with_dt(0.0);

        assert!(!a.blink_on, "blink should have toggled off");
        assert!(!a.is_visible(), "cursor should be invisible when blink is off");
    }

    #[test]
    fn blink_resets_on_cursor_move() {
        let mut a = CursorAnimator::new();
        // Force blink off
        thread::sleep(Duration::from_millis(550));
        a.update_with_dt(0.0);
        assert!(!a.blink_on);

        // Move cursor -- blink should reset to on
        a.set_target(100.0, 100.0, 8.0, 16.0, 0, [1.0; 4]);
        assert!(a.blink_on, "blink should reset to on after cursor move");
    }

    #[test]
    fn visibility_depends_on_visible_flag() {
        let mut a = CursorAnimator::new();
        assert!(a.is_visible());
        a.visible = false;
        assert!(!a.is_visible(), "should be invisible when visible=false");
    }

    // -----------------------------------------------------------------------
    // set_mode clears effects
    // -----------------------------------------------------------------------

    #[test]
    fn set_mode_clears_particles_and_rings() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Railgun);
        // Trigger particle spawn by moving
        a.set_target(100.0, 100.0, 8.0, 16.0, 0, [1.0; 4]);
        assert!(!a.particles.is_empty(), "railgun should spawn particles");

        a.set_mode(CursorAnimationMode::Smooth);
        assert!(a.particles.is_empty(), "set_mode should clear particles");
        assert!(a.rings.is_empty(), "set_mode should clear rings");
        assert!(a.trail.is_empty(), "set_mode should clear trail");
        assert_eq!(a.mode, CursorAnimationMode::Smooth);
    }

    // -----------------------------------------------------------------------
    // set_animation_speed clamping
    // -----------------------------------------------------------------------

    #[test]
    fn set_animation_speed_clamps() {
        let mut a = CursorAnimator::new();
        a.set_animation_speed(0.5);
        assert_eq!(a.animation_speed, 1.0, "speed below 1 should clamp to 1");

        a.set_animation_speed(200.0);
        assert_eq!(a.animation_speed, 100.0, "speed above 100 should clamp to 100");

        a.set_animation_speed(50.0);
        assert_eq!(a.animation_speed, 50.0);
    }

    // -----------------------------------------------------------------------
    // set_particle_count clamping
    // -----------------------------------------------------------------------

    #[test]
    fn set_particle_count_clamps() {
        let mut a = CursorAnimator::new();
        a.set_particle_count(0);
        assert_eq!(a.particle_count, 1, "count 0 should clamp to 1");

        a.set_particle_count(999);
        assert_eq!(a.particle_count, 100, "count above 100 should clamp to 100");

        a.set_particle_count(42);
        assert_eq!(a.particle_count, 42);
    }

    // -----------------------------------------------------------------------
    // Particle effects per mode
    // -----------------------------------------------------------------------

    #[test]
    fn railgun_spawns_particles_on_move() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Railgun);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);

        assert_eq!(a.particles.len(), a.particle_count as usize);
    }

    #[test]
    fn pixiedust_spawns_particles_on_move() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Pixiedust);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);

        assert_eq!(a.particles.len(), a.particle_count as usize);
    }

    #[test]
    fn sonicboom_spawns_ring_on_move() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Sonicboom);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);

        assert_eq!(a.rings.len(), 1);
    }

    #[test]
    fn ripple_spawns_three_rings_on_move() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Ripple);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);

        assert_eq!(a.rings.len(), 3, "ripple should spawn 3 concentric rings");
    }

    #[test]
    fn torpedo_adds_trail_point_on_move() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Torpedo);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);

        assert!(!a.trail.is_empty(), "torpedo should add trail point on move");
    }

    #[test]
    fn torpedo_adds_trail_points_while_animating() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Torpedo);
        a.set_target(500.0, 500.0, 8.0, 16.0, 0, [1.0; 4]);
        let initial_count = a.trail.len();

        // Tick several frames while still animating
        for _ in 0..5 {
            a.update_with_dt(0.016);
        }
        assert!(a.trail.len() > initial_count, "torpedo should accumulate trail points during animation");
    }

    #[test]
    fn smooth_mode_no_particles_or_rings() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Smooth);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);

        assert!(a.particles.is_empty(), "smooth mode should not spawn particles");
        assert!(a.rings.is_empty(), "smooth mode should not spawn rings");
    }

    #[test]
    fn wireframe_mode_no_particles_or_rings() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Wireframe);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);

        assert!(a.particles.is_empty(), "wireframe mode should not spawn particles");
        assert!(a.rings.is_empty(), "wireframe mode should not spawn rings");
    }

    // -----------------------------------------------------------------------
    // Particles/rings expire and are cleaned up
    // -----------------------------------------------------------------------

    #[test]
    fn particles_expire_after_lifetime() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Pixiedust);
        // Use very short particle lifetime
        a.particle_lifetime = Duration::from_millis(10);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);
        assert!(!a.particles.is_empty());

        thread::sleep(Duration::from_millis(20));
        a.update_with_dt(0.0);
        assert!(a.particles.is_empty(), "particles should be removed after lifetime");
    }

    #[test]
    fn rings_expire_after_lifetime() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Sonicboom);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);
        assert!(!a.rings.is_empty());

        // Sonicboom rings have 300ms lifetime
        thread::sleep(Duration::from_millis(350));
        a.update_with_dt(0.0);
        assert!(a.rings.is_empty(), "rings should be removed after lifetime");
    }

    // -----------------------------------------------------------------------
    // Trail max length
    // -----------------------------------------------------------------------

    #[test]
    fn trail_capped_at_max_length() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Torpedo);

        // Add many trail points by repeatedly moving
        for i in 0..60 {
            let x = (i as f32) * 10.0;
            a.set_target(x, 0.0, 8.0, 16.0, 0, [1.0; 4]);
        }

        assert!(a.trail.len() <= a.max_trail_length,
            "trail length {} should not exceed max {}", a.trail.len(), a.max_trail_length);
    }

    // -----------------------------------------------------------------------
    // is_animating reflects all effect sources
    // -----------------------------------------------------------------------

    #[test]
    fn is_animating_reflects_particles() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Railgun);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);
        assert!(a.is_animating(), "should be animating with active particles");
    }

    #[test]
    fn is_animating_reflects_rings() {
        let mut a = CursorAnimator::new();
        a.set_mode(CursorAnimationMode::Sonicboom);
        a.set_target(200.0, 200.0, 8.0, 16.0, 0, [1.0; 4]);
        assert!(a.is_animating(), "should be animating with active rings");
    }

    // -----------------------------------------------------------------------
    // Higher animation_speed converges faster
    // -----------------------------------------------------------------------

    #[test]
    fn higher_speed_converges_faster() {
        let mut slow = CursorAnimator::new();
        slow.set_animation_speed(5.0);
        slow.set_target(200.0, 0.0, 8.0, 16.0, 0, [1.0; 4]);

        let mut fast = CursorAnimator::new();
        fast.set_animation_speed(50.0);
        fast.set_target(200.0, 0.0, 8.0, 16.0, 0, [1.0; 4]);

        // Simulate 5 frames
        for _ in 0..5 {
            slow.update_with_dt(0.016);
            fast.update_with_dt(0.016);
        }

        assert!(fast.current_x > slow.current_x,
            "faster animator ({}) should be closer to target than slower ({})",
            fast.current_x, slow.current_x);
    }

    // -----------------------------------------------------------------------
    // Width/height also animate smoothly
    // -----------------------------------------------------------------------

    #[test]
    fn width_and_height_animate() {
        let mut a = CursorAnimator::new();
        // Default width=8, height=16; change to 20x40
        a.set_target(0.0, 0.0, 20.0, 40.0, 0, [1.0; 4]);
        a.update_with_dt(0.016);

        assert!(a.current_width > 8.0, "width should animate toward 20");
        assert!(a.current_height > 16.0, "height should animate toward 40");

        // Converge
        for _ in 0..200 {
            a.update_with_dt(0.016);
        }
        assert!((a.current_width - 20.0).abs() < 0.5);
        assert!((a.current_height - 40.0).abs() < 0.5);
    }

    // -----------------------------------------------------------------------
    // update() (wall-clock based) basic smoke test
    // -----------------------------------------------------------------------

    #[test]
    fn update_wall_clock_smoke_test() {
        let mut a = CursorAnimator::new();
        a.set_target(100.0, 100.0, 8.0, 16.0, 0, [1.0; 4]);
        thread::sleep(Duration::from_millis(16));
        let active = a.update();
        // Should be animating since we just moved
        assert!(active);
        assert!(a.current_x > 0.0, "should have moved from origin");
    }
}
