
struct Hillshading {
    tile_size: usize,
    altitude: f32,
    azimuth: f32,
    intensity: f32,
}

impl Hillshading {
    pub fn new(tile_size: usize) -> Self {
        Self {
            tile_size
            altitude: 25.0_f32.to_radians(),
            azimuth: -45.0_f32.to_radians(),
            intensity: 1,
        }
    }
    
    
}