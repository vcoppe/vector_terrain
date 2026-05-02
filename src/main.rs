mod elevation_reader;

use elevation_reader::ElevationReader;
use pmtiles::TileCoord;
use oxigdal_core::buffer::RasterBuffer;
use image::{ImageBuffer, Luma};
use oxigdal_algorithms::raster::swiss_hillshade;

const TILE_SIZE: usize = 512;

fn save_array_as_png(data: &RasterBuffer, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut pixels = vec![0; TILE_SIZE * TILE_SIZE];
    for i in 0..TILE_SIZE {
        for j in 0..TILE_SIZE {
            pixels[i * TILE_SIZE + j] = data.get_pixel(i as u64, j as u64).unwrap() as u8;
        }
    }
    let img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(TILE_SIZE as u32, TILE_SIZE as u32, pixels)
        .ok_or("Failed to create image buffer")?;
    img.save(filename)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let file = "planet.pmtiles";
    let reader = ElevationReader::new(file, TILE_SIZE).await;
    
    let coord = TileCoord::new(12, 2078, 1554).unwrap();
    let elevation = reader.get(coord).await;
    let hillshade = swiss_hillshade(&elevation, 1.0, 30.0).unwrap();
    save_array_as_png(&hillshade, "hillshade.png");
    println!("{:?}", hillshade);
}