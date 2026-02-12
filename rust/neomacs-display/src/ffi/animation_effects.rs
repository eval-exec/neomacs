
/// Configure heat distortion effect
#[no_mangle]
effect_setter!(neomacs_display_set_heat_distortion(enabled: c_int, intensity: c_int, speed: c_int, edge_width: c_int, opacity: c_int) |effects| {
        effects.heat_distortion.enabled = enabled != 0;
                    effects.heat_distortion.intensity = intensity as f32 / 100.0;
                    effects.heat_distortion.speed = speed as f32 / 100.0;
                    effects.heat_distortion.edge_width = edge_width as f32;
                    effects.heat_distortion.opacity = opacity as f32 / 100.0;
});


/// Configure cursor lighthouse beam effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_lighthouse(enabled: c_int, r: c_int, g: c_int, b: c_int, beam_width: c_int, rotation_speed: c_int, beam_length: c_int, opacity: c_int) |effects| {
        effects.cursor_lighthouse.enabled = enabled != 0;
                    effects.cursor_lighthouse.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_lighthouse.beam_width = beam_width as f32;
                    effects.cursor_lighthouse.rotation_speed = rotation_speed as f32 / 100.0;
                    effects.cursor_lighthouse.beam_length = beam_length as f32;
                    effects.cursor_lighthouse.opacity = opacity as f32 / 100.0;
});


/// Configure neon border effect
#[no_mangle]
effect_setter!(neomacs_display_set_neon_border(enabled: c_int, r: c_int, g: c_int, b: c_int, intensity: c_int, flicker: c_int, thickness: c_int, opacity: c_int) |effects| {
        effects.neon_border.enabled = enabled != 0;
                    effects.neon_border.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.neon_border.intensity = intensity as f32 / 100.0;
                    effects.neon_border.flicker = flicker as f32 / 100.0;
                    effects.neon_border.thickness = thickness as f32;
                    effects.neon_border.opacity = opacity as f32 / 100.0;
});


/// Configure cursor sonar ping effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_sonar_ping(enabled: c_int, r: c_int, g: c_int, b: c_int, ring_count: c_int, max_radius: c_int, duration_ms: c_int, opacity: c_int) |effects| {
        effects.cursor_sonar_ping.enabled = enabled != 0;
                    effects.cursor_sonar_ping.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_sonar_ping.ring_count = ring_count as u32;
                    effects.cursor_sonar_ping.max_radius = max_radius as f32;
                    effects.cursor_sonar_ping.duration_ms = duration_ms as u32;
                    effects.cursor_sonar_ping.opacity = opacity as f32 / 100.0;
});


/// Configure lightning bolt effect
#[no_mangle]
effect_setter!(neomacs_display_set_lightning_bolt(enabled: c_int, r: c_int, g: c_int, b: c_int, frequency: c_int, intensity: c_int, opacity: c_int) |effects| {
        effects.lightning_bolt.enabled = enabled != 0;
                    effects.lightning_bolt.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.lightning_bolt.frequency = frequency as f32 / 100.0;
                    effects.lightning_bolt.intensity = intensity as f32 / 100.0;
                    effects.lightning_bolt.opacity = opacity as f32 / 100.0;
});


/// Configure cursor orbit particles effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_orbit_particles(enabled: c_int, r: c_int, g: c_int, b: c_int, particle_count: c_int, orbit_radius: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.cursor_orbit_particles.enabled = enabled != 0;
                    effects.cursor_orbit_particles.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_orbit_particles.count = particle_count as u32;
                    effects.cursor_orbit_particles.radius = orbit_radius as f32;
                    effects.cursor_orbit_particles.speed = speed as f32 / 100.0;
                    effects.cursor_orbit_particles.opacity = opacity as f32 / 100.0;
});


/// Configure plasma border effect
#[no_mangle]
effect_setter!(neomacs_display_set_plasma_border(enabled: c_int, r1: c_int, g1: c_int, b1: c_int, r2: c_int, g2: c_int, b2: c_int, width: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.plasma_border.enabled = enabled != 0;
                    effects.plasma_border.color1 = (r1 as f32 / 255.0, g1 as f32 / 255.0, b1 as f32 / 255.0);
                    effects.plasma_border.color2 = (r2 as f32 / 255.0, g2 as f32 / 255.0, b2 as f32 / 255.0);
                    effects.plasma_border.width = width as f32;
                    effects.plasma_border.speed = speed as f32 / 100.0;
                    effects.plasma_border.opacity = opacity as f32 / 100.0;
});


/// Configure cursor heartbeat pulse effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_heartbeat(enabled: c_int, r: c_int, g: c_int, b: c_int, bpm: c_int, max_radius: c_int, opacity: c_int) |effects| {
        effects.cursor_heartbeat.enabled = enabled != 0;
                    effects.cursor_heartbeat.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_heartbeat.bpm = bpm as f32;
                    effects.cursor_heartbeat.max_radius = max_radius as f32;
                    effects.cursor_heartbeat.opacity = opacity as f32 / 100.0;
});


/// Configure topographic contour effect
#[no_mangle]
effect_setter!(neomacs_display_set_topo_contour(enabled: c_int, r: c_int, g: c_int, b: c_int, spacing: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.topo_contour.enabled = enabled != 0;
                    effects.topo_contour.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.topo_contour.spacing = spacing as f32;
                    effects.topo_contour.speed = speed as f32 / 100.0;
                    effects.topo_contour.opacity = opacity as f32 / 100.0;
});


/// Configure cursor metronome tick effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_metronome(enabled: c_int, r: c_int, g: c_int, b: c_int, tick_height: c_int, fade_ms: c_int, opacity: c_int) |effects| {
        effects.cursor_metronome.enabled = enabled != 0;
                    effects.cursor_metronome.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_metronome.tick_height = tick_height as f32;
                    effects.cursor_metronome.fade_ms = fade_ms as u32;
                    effects.cursor_metronome.opacity = opacity as f32 / 100.0;
});


/// Configure constellation overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_constellation(enabled: c_int, r: c_int, g: c_int, b: c_int, star_count: c_int, connect_dist: c_int, twinkle_speed: c_int, opacity: c_int) |effects| {
        effects.constellation.enabled = enabled != 0;
                    effects.constellation.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.constellation.star_count = star_count as u32;
                    effects.constellation.connect_dist = connect_dist as f32;
                    effects.constellation.twinkle_speed = twinkle_speed as f32 / 100.0;
                    effects.constellation.opacity = opacity as f32 / 100.0;
});


/// Configure cursor radar sweep effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_radar(enabled: c_int, r: c_int, g: c_int, b: c_int, radius: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.cursor_radar.enabled = enabled != 0;
                    effects.cursor_radar.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_radar.radius = radius as f32;
                    effects.cursor_radar.speed = speed as f32 / 100.0;
                    effects.cursor_radar.opacity = opacity as f32 / 100.0;
});


/// Configure hex grid overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_hex_grid(enabled: c_int, r: c_int, g: c_int, b: c_int, cell_size: c_int, pulse_speed: c_int, opacity: c_int) |effects| {
        effects.hex_grid.enabled = enabled != 0;
                    effects.hex_grid.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.hex_grid.cell_size = cell_size as f32;
                    effects.hex_grid.pulse_speed = pulse_speed as f32 / 100.0;
                    effects.hex_grid.opacity = opacity as f32 / 100.0;
});


/// Configure cursor sparkle burst effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_sparkle_burst(enabled: c_int, r: c_int, g: c_int, b: c_int, particle_count: c_int, burst_radius: c_int, opacity: c_int) |effects| {
        effects.cursor_sparkle_burst.enabled = enabled != 0;
                    effects.cursor_sparkle_burst.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_sparkle_burst.count = particle_count as u32;
                    effects.cursor_sparkle_burst.radius = burst_radius as f32;
                    effects.cursor_sparkle_burst.opacity = opacity as f32 / 100.0;
});


/// Configure circuit board trace effect
#[no_mangle]
effect_setter!(neomacs_display_set_circuit_trace(enabled: c_int, r: c_int, g: c_int, b: c_int, trace_width: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.circuit_trace.enabled = enabled != 0;
                    effects.circuit_trace.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.circuit_trace.width = trace_width as f32;
                    effects.circuit_trace.speed = speed as f32 / 100.0;
                    effects.circuit_trace.opacity = opacity as f32 / 100.0;
});


/// Configure cursor compass rose effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_compass(enabled: c_int, r: c_int, g: c_int, b: c_int, size: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.cursor_compass.enabled = enabled != 0;
                    effects.cursor_compass.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_compass.size = size as f32;
                    effects.cursor_compass.speed = speed as f32 / 100.0;
                    effects.cursor_compass.opacity = opacity as f32 / 100.0;
});


/// Configure warp/distortion grid effect
#[no_mangle]
effect_setter!(neomacs_display_set_warp_grid(enabled: c_int, r: c_int, g: c_int, b: c_int, density: c_int, amplitude: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.warp_grid.enabled = enabled != 0;
                    effects.warp_grid.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.warp_grid.density = density as u32;
                    effects.warp_grid.amplitude = amplitude as f32;
                    effects.warp_grid.speed = speed as f32 / 100.0;
                    effects.warp_grid.opacity = opacity as f32 / 100.0;
});


/// Configure cursor DNA helix trail effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_dna_helix(enabled: c_int, r1: c_int, g1: c_int, b1: c_int, r2: c_int, g2: c_int, b2: c_int, radius: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.cursor_dna_helix.enabled = enabled != 0;
                    effects.cursor_dna_helix.color1 = (r1 as f32 / 255.0, g1 as f32 / 255.0, b1 as f32 / 255.0);
                    effects.cursor_dna_helix.color2 = (r2 as f32 / 255.0, g2 as f32 / 255.0, b2 as f32 / 255.0);
                    effects.cursor_dna_helix.radius = radius as f32;
                    effects.cursor_dna_helix.speed = speed as f32 / 100.0;
                    effects.cursor_dna_helix.opacity = opacity as f32 / 100.0;
});


/// Configure prism/rainbow edge effect
#[no_mangle]
effect_setter!(neomacs_display_set_prism_edge(enabled: c_int, width: c_int, speed: c_int, saturation: c_int, opacity: c_int) |effects| {
        effects.prism_edge.enabled = enabled != 0;
                    effects.prism_edge.width = width as f32;
                    effects.prism_edge.speed = speed as f32 / 100.0;
                    effects.prism_edge.saturation = saturation as f32 / 100.0;
                    effects.prism_edge.opacity = opacity as f32 / 100.0;
});


/// Configure cursor pendulum swing effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_pendulum(enabled: c_int, r: c_int, g: c_int, b: c_int, arc_length: c_int, damping: c_int, opacity: c_int) |effects| {
        effects.cursor_pendulum.enabled = enabled != 0;
                    effects.cursor_pendulum.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_pendulum.arc_length = arc_length as f32;
                    effects.cursor_pendulum.damping = damping as f32 / 100.0;
                    effects.cursor_pendulum.opacity = opacity as f32 / 100.0;
});


/// Configure kaleidoscope overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_kaleidoscope(enabled: c_int, r: c_int, g: c_int, b: c_int, segments: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.kaleidoscope.enabled = enabled != 0;
                    effects.kaleidoscope.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.kaleidoscope.segments = segments as u32;
                    effects.kaleidoscope.speed = speed as f32 / 100.0;
                    effects.kaleidoscope.opacity = opacity as f32 / 100.0;
});


/// Configure cursor ripple ring effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_ripple_ring(enabled: c_int, r: c_int, g: c_int, b: c_int, max_radius: c_int, ring_count: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.cursor_ripple_ring.enabled = enabled != 0;
                    effects.cursor_ripple_ring.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_ripple_ring.max_radius = max_radius as f32;
                    effects.cursor_ripple_ring.count = ring_count as u32;
                    effects.cursor_ripple_ring.speed = speed as f32 / 100.0;
                    effects.cursor_ripple_ring.opacity = opacity as f32 / 100.0;
});


/// Configure noise field overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_noise_field(enabled: c_int, r: c_int, g: c_int, b: c_int, scale: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.noise_field.enabled = enabled != 0;
                    effects.noise_field.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.noise_field.scale = scale as f32;
                    effects.noise_field.speed = speed as f32 / 100.0;
                    effects.noise_field.opacity = opacity as f32 / 100.0;
});


/// Configure cursor scope effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_scope(enabled: c_int, r: c_int, g: c_int, b: c_int, thickness: c_int, gap: c_int, opacity: c_int) |effects| {
        effects.cursor_scope.enabled = enabled != 0;
                    effects.cursor_scope.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_scope.thickness = thickness as f32;
                    effects.cursor_scope.gap = gap as f32;
                    effects.cursor_scope.opacity = opacity as f32 / 100.0;
});


/// Configure spiral vortex overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_spiral_vortex(enabled: c_int, r: c_int, g: c_int, b: c_int, arms: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.spiral_vortex.enabled = enabled != 0;
                    effects.spiral_vortex.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.spiral_vortex.arms = arms as u32;
                    effects.spiral_vortex.speed = speed as f32 / 100.0;
                    effects.spiral_vortex.opacity = opacity as f32 / 100.0;
});


/// Configure cursor shockwave effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_shockwave(enabled: c_int, r: c_int, g: c_int, b: c_int, radius: c_int, decay: c_int, opacity: c_int) |effects| {
        effects.cursor_shockwave.enabled = enabled != 0;
                    effects.cursor_shockwave.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_shockwave.radius = radius as f32;
                    effects.cursor_shockwave.decay = decay as f32 / 100.0;
                    effects.cursor_shockwave.opacity = opacity as f32 / 100.0;
});


/// Configure diamond lattice overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_diamond_lattice(enabled: c_int, r: c_int, g: c_int, b: c_int, cell_size: c_int, shimmer_speed: c_int, opacity: c_int) |effects| {
        effects.diamond_lattice.enabled = enabled != 0;
                    effects.diamond_lattice.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.diamond_lattice.cell_size = cell_size as f32;
                    effects.diamond_lattice.shimmer_speed = shimmer_speed as f32 / 100.0;
                    effects.diamond_lattice.opacity = opacity as f32 / 100.0;
});


/// Configure cursor gravity well effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_gravity_well(enabled: c_int, r: c_int, g: c_int, b: c_int, field_radius: c_int, line_count: c_int, opacity: c_int) |effects| {
        effects.cursor_gravity_well.enabled = enabled != 0;
                    effects.cursor_gravity_well.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_gravity_well.field_radius = field_radius as f32;
                    effects.cursor_gravity_well.line_count = line_count as u32;
                    effects.cursor_gravity_well.opacity = opacity as f32 / 100.0;
});


/// Configure tessellation overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_tessellation(enabled: c_int, r: c_int, g: c_int, b: c_int, tile_size: c_int, rotation: c_int, opacity: c_int) |effects| {
        effects.tessellation.enabled = enabled != 0;
                    effects.tessellation.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.tessellation.tile_size = tile_size as f32;
                    effects.tessellation.rotation = rotation as f32 / 100.0;
                    effects.tessellation.opacity = opacity as f32 / 100.0;
});


/// Configure cursor water drop effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_water_drop(enabled: c_int, r: c_int, g: c_int, b: c_int, ripple_count: c_int, expand_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_water_drop.enabled = enabled != 0;
                    effects.cursor_water_drop.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_water_drop.ripple_count = ripple_count as u32;
                    effects.cursor_water_drop.expand_speed = expand_speed as f32 / 100.0;
                    effects.cursor_water_drop.opacity = opacity as f32 / 100.0;
});


/// Configure guilloche overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_guilloche(enabled: c_int, r: c_int, g: c_int, b: c_int, curve_count: c_int, wave_freq: c_int, opacity: c_int) |effects| {
        effects.guilloche.enabled = enabled != 0;
                    effects.guilloche.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.guilloche.curve_count = curve_count as u32;
                    effects.guilloche.wave_freq = wave_freq as f32 / 100.0;
                    effects.guilloche.opacity = opacity as f32 / 100.0;
});


/// Configure cursor pixel dust effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_pixel_dust(enabled: c_int, r: c_int, g: c_int, b: c_int, dust_count: c_int, scatter_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_pixel_dust.enabled = enabled != 0;
                    effects.cursor_pixel_dust.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_pixel_dust.count = dust_count as u32;
                    effects.cursor_pixel_dust.scatter_speed = scatter_speed as f32 / 100.0;
                    effects.cursor_pixel_dust.opacity = opacity as f32 / 100.0;
});


/// Configure celtic knot overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_celtic_knot(enabled: c_int, r: c_int, g: c_int, b: c_int, knot_scale: c_int, weave_speed: c_int, opacity: c_int) |effects| {
        effects.celtic_knot.enabled = enabled != 0;
                    effects.celtic_knot.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.celtic_knot.scale = knot_scale as f32;
                    effects.celtic_knot.weave_speed = weave_speed as f32 / 100.0;
                    effects.celtic_knot.opacity = opacity as f32 / 100.0;
});


/// Configure cursor candle flame effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_candle_flame(enabled: c_int, r: c_int, g: c_int, b: c_int, flame_height: c_int, flicker_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_candle_flame.enabled = enabled != 0;
                    effects.cursor_candle_flame.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_candle_flame.height = flame_height as u32;
                    effects.cursor_candle_flame.flicker_speed = flicker_speed as f32 / 100.0;
                    effects.cursor_candle_flame.opacity = opacity as f32 / 100.0;
});


/// Configure argyle pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_argyle_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, diamond_size: c_int, line_width: c_int, opacity: c_int) |effects| {
        effects.argyle_pattern.enabled = enabled != 0;
                    effects.argyle_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.argyle_pattern.diamond_size = diamond_size as f32;
                    effects.argyle_pattern.line_width = line_width as f32;
                    effects.argyle_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor moth flame effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_moth_flame(enabled: c_int, r: c_int, g: c_int, b: c_int, moth_count: c_int, orbit_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_moth_flame.enabled = enabled != 0;
                    effects.cursor_moth_flame.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_moth_flame.moth_count = moth_count as u32;
                    effects.cursor_moth_flame.orbit_speed = orbit_speed as f32 / 100.0;
                    effects.cursor_moth_flame.opacity = opacity as f32 / 100.0;
});


/// Configure basket weave overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_basket_weave(enabled: c_int, r: c_int, g: c_int, b: c_int, strip_width: c_int, strip_spacing: c_int, opacity: c_int) |effects| {
        effects.basket_weave.enabled = enabled != 0;
                    effects.basket_weave.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.basket_weave.strip_width = strip_width as f32;
                    effects.basket_weave.strip_spacing = strip_spacing as f32;
                    effects.basket_weave.opacity = opacity as f32 / 100.0;
});


/// Configure cursor sparkler effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_sparkler(enabled: c_int, r: c_int, g: c_int, b: c_int, spark_count: c_int, burn_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_sparkler.enabled = enabled != 0;
                    effects.cursor_sparkler.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_sparkler.spark_count = spark_count as u32;
                    effects.cursor_sparkler.burn_speed = burn_speed as f32 / 100.0;
                    effects.cursor_sparkler.opacity = opacity as f32 / 100.0;
});


/// Configure fish scale overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_fish_scale(enabled: c_int, r: c_int, g: c_int, b: c_int, scale_size: c_int, row_offset: c_int, opacity: c_int) |effects| {
        effects.fish_scale.enabled = enabled != 0;
                    effects.fish_scale.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.fish_scale.size = scale_size as f32;
                    effects.fish_scale.row_offset = row_offset as f32 / 100.0;
                    effects.fish_scale.opacity = opacity as f32 / 100.0;
});


/// Configure cursor plasma ball effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_plasma_ball(enabled: c_int, r: c_int, g: c_int, b: c_int, tendril_count: c_int, arc_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_plasma_ball.enabled = enabled != 0;
                    effects.cursor_plasma_ball.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_plasma_ball.tendril_count = tendril_count as u32;
                    effects.cursor_plasma_ball.arc_speed = arc_speed as f32 / 100.0;
                    effects.cursor_plasma_ball.opacity = opacity as f32 / 100.0;
});


/// Configure trefoil knot overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_trefoil_knot(enabled: c_int, r: c_int, g: c_int, b: c_int, knot_size: c_int, rotation_speed: c_int, opacity: c_int) |effects| {
        effects.trefoil_knot.enabled = enabled != 0;
                    effects.trefoil_knot.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.trefoil_knot.size = knot_size as f32;
                    effects.trefoil_knot.rotation_speed = rotation_speed as f32 / 100.0;
                    effects.trefoil_knot.opacity = opacity as f32 / 100.0;
});


/// Configure cursor quill pen effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_quill_pen(enabled: c_int, r: c_int, g: c_int, b: c_int, trail_length: c_int, ink_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_quill_pen.enabled = enabled != 0;
                    effects.cursor_quill_pen.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_quill_pen.trail_length = trail_length as u32;
                    effects.cursor_quill_pen.ink_speed = ink_speed as f32 / 100.0;
                    effects.cursor_quill_pen.opacity = opacity as f32 / 100.0;
});


/// Configure herringbone pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_herringbone_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, tile_width: c_int, tile_height: c_int, opacity: c_int) |effects| {
        effects.herringbone_pattern.enabled = enabled != 0;
                    effects.herringbone_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.herringbone_pattern.tile_width = tile_width as f32;
                    effects.herringbone_pattern.tile_height = tile_height as f32;
                    effects.herringbone_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor aurora borealis effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_aurora_borealis(enabled: c_int, r: c_int, g: c_int, b: c_int, band_count: c_int, shimmer_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_aurora_borealis.enabled = enabled != 0;
                    effects.cursor_aurora_borealis.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_aurora_borealis.band_count = band_count as u32;
                    effects.cursor_aurora_borealis.shimmer_speed = shimmer_speed as f32 / 100.0;
                    effects.cursor_aurora_borealis.opacity = opacity as f32 / 100.0;
});


/// Configure target reticle overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_target_reticle(enabled: c_int, r: c_int, g: c_int, b: c_int, ring_count: c_int, pulse_speed: c_int, opacity: c_int) |effects| {
        effects.target_reticle.enabled = enabled != 0;
                    effects.target_reticle.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.target_reticle.ring_count = ring_count as u32;
                    effects.target_reticle.pulse_speed = pulse_speed as f32 / 100.0;
                    effects.target_reticle.opacity = opacity as f32 / 100.0;
});


/// Configure cursor feather effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_feather(enabled: c_int, r: c_int, g: c_int, b: c_int, feather_count: c_int, drift_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_feather.enabled = enabled != 0;
                    effects.cursor_feather.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_feather.count = feather_count as u32;
                    effects.cursor_feather.drift_speed = drift_speed as f32 / 100.0;
                    effects.cursor_feather.opacity = opacity as f32 / 100.0;
});


/// Configure plaid pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_plaid_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, band_width: c_int, band_spacing: c_int, opacity: c_int) |effects| {
        effects.plaid_pattern.enabled = enabled != 0;
                    effects.plaid_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.plaid_pattern.band_width = band_width as f32;
                    effects.plaid_pattern.band_spacing = band_spacing as f32;
                    effects.plaid_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor stardust effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_stardust(enabled: c_int, r: c_int, g: c_int, b: c_int, particle_count: c_int, fall_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_stardust.enabled = enabled != 0;
                    effects.cursor_stardust.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_stardust.particle_count = particle_count as u32;
                    effects.cursor_stardust.fall_speed = fall_speed as f32 / 100.0;
                    effects.cursor_stardust.opacity = opacity as f32 / 100.0;
});


/// Configure brick wall overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_brick_wall(enabled: c_int, r: c_int, g: c_int, b: c_int, brick_width: c_int, brick_height: c_int, opacity: c_int) |effects| {
        effects.brick_wall.enabled = enabled != 0;
                    effects.brick_wall.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.brick_wall.width = brick_width as f32;
                    effects.brick_wall.height = brick_height as f32;
                    effects.brick_wall.opacity = opacity as f32 / 100.0;
});


/// Configure cursor compass needle effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_compass_needle(enabled: c_int, r: c_int, g: c_int, b: c_int, needle_length: c_int, spin_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_compass_needle.enabled = enabled != 0;
                    effects.cursor_compass_needle.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_compass_needle.length = needle_length as f32;
                    effects.cursor_compass_needle.spin_speed = spin_speed as f32 / 100.0;
                    effects.cursor_compass_needle.opacity = opacity as f32 / 100.0;
});


/// Configure sine wave overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_sine_wave(enabled: c_int, r: c_int, g: c_int, b: c_int, amplitude: c_int, wavelength: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.sine_wave.enabled = enabled != 0;
                    effects.sine_wave.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.sine_wave.amplitude = amplitude as f32;
                    effects.sine_wave.wavelength = wavelength as f32;
                    effects.sine_wave.speed = speed as f32 / 100.0;
                    effects.sine_wave.opacity = opacity as f32 / 100.0;
});


/// Configure cursor galaxy effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_galaxy(enabled: c_int, r: c_int, g: c_int, b: c_int, star_count: c_int, radius: c_int, opacity: c_int) |effects| {
        effects.cursor_galaxy.enabled = enabled != 0;
                    effects.cursor_galaxy.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_galaxy.star_count = star_count as u32;
                    effects.cursor_galaxy.radius = radius as f32;
                    effects.cursor_galaxy.opacity = opacity as f32 / 100.0;
});


/// Configure rotating gear overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_rotating_gear(enabled: c_int, r: c_int, g: c_int, b: c_int, gear_size: c_int, rotation_speed: c_int, opacity: c_int) |effects| {
        effects.rotating_gear.enabled = enabled != 0;
                    effects.rotating_gear.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.rotating_gear.size = gear_size as f32;
                    effects.rotating_gear.speed = rotation_speed as f32 / 100.0;
                    effects.rotating_gear.opacity = opacity as f32 / 100.0;
});


/// Configure cursor prism effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_prism(enabled: c_int, r: c_int, g: c_int, b: c_int, ray_count: c_int, spread: c_int, opacity: c_int) |effects| {
        effects.cursor_prism.enabled = enabled != 0;
                    effects.cursor_prism.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_prism.ray_count = ray_count as u32;
                    effects.cursor_prism.spread = spread as f32;
                    effects.cursor_prism.opacity = opacity as f32 / 100.0;
});


/// Configure crosshatch pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_crosshatch_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, line_spacing: c_int, angle: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.crosshatch_pattern.enabled = enabled != 0;
                    effects.crosshatch_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.crosshatch_pattern.line_spacing = line_spacing as f32;
                    effects.crosshatch_pattern.angle = angle as f32;
                    effects.crosshatch_pattern.speed = speed as f32 / 100.0;
                    effects.crosshatch_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor moth effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_moth(enabled: c_int, r: c_int, g: c_int, b: c_int, moth_count: c_int, wing_size: c_int, opacity: c_int) |effects| {
        effects.cursor_moth.enabled = enabled != 0;
                    effects.cursor_moth.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_moth.count = moth_count as u32;
                    effects.cursor_moth.wing_size = wing_size as f32;
                    effects.cursor_moth.opacity = opacity as f32 / 100.0;
});


/// Configure concentric rings overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_concentric_rings(enabled: c_int, r: c_int, g: c_int, b: c_int, ring_spacing: c_int, expansion_speed: c_int, opacity: c_int) |effects| {
        effects.concentric_rings.enabled = enabled != 0;
                    effects.concentric_rings.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.concentric_rings.spacing = ring_spacing as f32;
                    effects.concentric_rings.expansion_speed = expansion_speed as f32 / 100.0;
                    effects.concentric_rings.opacity = opacity as f32 / 100.0;
});


/// Configure cursor flame effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_flame(enabled: c_int, r: c_int, g: c_int, b: c_int, particle_count: c_int, height: c_int, opacity: c_int) |effects| {
        effects.cursor_flame.enabled = enabled != 0;
                    effects.cursor_flame.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_flame.particle_count = particle_count as u32;
                    effects.cursor_flame.height = height as f32;
                    effects.cursor_flame.opacity = opacity as f32 / 100.0;
});


/// Configure zigzag pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_zigzag_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, amplitude: c_int, frequency: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.zigzag_pattern.enabled = enabled != 0;
                    effects.zigzag_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.zigzag_pattern.amplitude = amplitude as f32;
                    effects.zigzag_pattern.frequency = frequency as f32 / 100.0;
                    effects.zigzag_pattern.speed = speed as f32 / 100.0;
                    effects.zigzag_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor crystal effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_crystal(enabled: c_int, r: c_int, g: c_int, b: c_int, facet_count: c_int, radius: c_int, opacity: c_int) |effects| {
        effects.cursor_crystal.enabled = enabled != 0;
                    effects.cursor_crystal.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_crystal.facet_count = facet_count as u32;
                    effects.cursor_crystal.radius = radius as f32;
                    effects.cursor_crystal.opacity = opacity as f32 / 100.0;
});


/// Configure moiré pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_moire_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, line_spacing: c_int, angle_offset: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.moire_pattern.enabled = enabled != 0;
                    effects.moire_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.moire_pattern.line_spacing = line_spacing as f32;
                    effects.moire_pattern.angle_offset = angle_offset as f32;
                    effects.moire_pattern.speed = speed as f32 / 100.0;
                    effects.moire_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor lightning effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_lightning(enabled: c_int, r: c_int, g: c_int, b: c_int, bolt_count: c_int, max_length: c_int, opacity: c_int) |effects| {
        effects.cursor_lightning.enabled = enabled != 0;
                    effects.cursor_lightning.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_lightning.bolt_count = bolt_count as u32;
                    effects.cursor_lightning.max_length = max_length as f32;
                    effects.cursor_lightning.opacity = opacity as f32 / 100.0;
});


/// Configure dot matrix overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_dot_matrix(enabled: c_int, r: c_int, g: c_int, b: c_int, dot_spacing: c_int, pulse_speed: c_int, opacity: c_int) |effects| {
        effects.dot_matrix.enabled = enabled != 0;
                    effects.dot_matrix.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.dot_matrix.spacing = dot_spacing as f32;
                    effects.dot_matrix.pulse_speed = pulse_speed as f32 / 100.0;
                    effects.dot_matrix.opacity = opacity as f32 / 100.0;
});


/// Configure cursor snowflake effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_snowflake(enabled: c_int, r: c_int, g: c_int, b: c_int, count: c_int, fall_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_snowflake.enabled = enabled != 0;
                    effects.cursor_snowflake.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_snowflake.count = count as u32;
                    effects.cursor_snowflake.fall_speed = fall_speed as f32;
                    effects.cursor_snowflake.opacity = opacity as f32 / 100.0;
});


/// Configure sunburst pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_sunburst_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, ray_count: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.sunburst_pattern.enabled = enabled != 0;
                    effects.sunburst_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.sunburst_pattern.ray_count = ray_count as u32;
                    effects.sunburst_pattern.speed = speed as f32 / 100.0;
                    effects.sunburst_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor firework effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_firework(enabled: c_int, r: c_int, g: c_int, b: c_int, particle_count: c_int, burst_radius: c_int, opacity: c_int) |effects| {
        effects.cursor_firework.enabled = enabled != 0;
                    effects.cursor_firework.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_firework.particle_count = particle_count as u32;
                    effects.cursor_firework.burst_radius = burst_radius as f32;
                    effects.cursor_firework.opacity = opacity as f32 / 100.0;
});


/// Configure honeycomb dissolve overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_honeycomb_dissolve(enabled: c_int, r: c_int, g: c_int, b: c_int, cell_size: c_int, dissolve_speed: c_int, opacity: c_int) |effects| {
        effects.honeycomb_dissolve.enabled = enabled != 0;
                    effects.honeycomb_dissolve.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.honeycomb_dissolve.cell_size = cell_size as f32;
                    effects.honeycomb_dissolve.speed = dissolve_speed as f32 / 100.0;
                    effects.honeycomb_dissolve.opacity = opacity as f32 / 100.0;
});


/// Configure cursor tornado effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_tornado(enabled: c_int, r: c_int, g: c_int, b: c_int, radius: c_int, particle_count: c_int, opacity: c_int) |effects| {
        effects.cursor_tornado.enabled = enabled != 0;
                    effects.cursor_tornado.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_tornado.radius = radius as f32;
                    effects.cursor_tornado.particle_count = particle_count as u32;
                    effects.cursor_tornado.opacity = opacity as f32 / 100.0;
});


/// Configure wave interference overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_wave_interference(enabled: c_int, r: c_int, g: c_int, b: c_int, wavelength: c_int, source_count: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.wave_interference.enabled = enabled != 0;
                    effects.wave_interference.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.wave_interference.wavelength = wavelength as f32;
                    effects.wave_interference.source_count = source_count as u32;
                    effects.wave_interference.speed = speed as f32 / 100.0;
                    effects.wave_interference.opacity = opacity as f32 / 100.0;
});


/// Configure cursor portal effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_portal(enabled: c_int, r: c_int, g: c_int, b: c_int, radius: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.cursor_portal.enabled = enabled != 0;
                    effects.cursor_portal.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_portal.radius = radius as f32;
                    effects.cursor_portal.speed = speed as f32 / 100.0;
                    effects.cursor_portal.opacity = opacity as f32 / 100.0;
});


/// Configure chevron pattern overlay effect
#[no_mangle]
effect_setter!(neomacs_display_set_chevron_pattern(enabled: c_int, r: c_int, g: c_int, b: c_int, spacing: c_int, speed: c_int, opacity: c_int) |effects| {
        effects.chevron_pattern.enabled = enabled != 0;
                    effects.chevron_pattern.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.chevron_pattern.spacing = spacing as f32;
                    effects.chevron_pattern.speed = speed as f32 / 100.0;
                    effects.chevron_pattern.opacity = opacity as f32 / 100.0;
});


/// Configure cursor bubble effect
#[no_mangle]
effect_setter!(neomacs_display_set_cursor_bubble(enabled: c_int, r: c_int, g: c_int, b: c_int, count: c_int, rise_speed: c_int, opacity: c_int) |effects| {
        effects.cursor_bubble.enabled = enabled != 0;
                    effects.cursor_bubble.color = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
                    effects.cursor_bubble.count = count as u32;
                    effects.cursor_bubble.rise_speed = rise_speed as f32;
                    effects.cursor_bubble.opacity = opacity as f32 / 100.0;
});

/// Configure Gaussian blur post-processing effect.
///
/// Cross-platform GPU blur using separable two-pass convolution.
/// `radius` is in percentage (100 = 1.0x), `passes` is the number
/// of blur iterations (more = stronger blur).
#[no_mangle]
effect_setter!(neomacs_display_set_blur(enabled: c_int, radius: c_int, passes: c_int) |effects| {
    effects.blur.enabled = enabled != 0;
    effects.blur.radius = radius as f32 / 100.0;
    effects.blur.passes = (passes as u32).max(1);
});
